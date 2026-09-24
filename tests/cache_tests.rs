use std::path::PathBuf;
use std::sync::Mutex;

use soroban_cost_estimator::cache::{self, CacheExport, CachedEstimate};

/// Serialize cache tests because `std::env::set_var` is not thread-safe.
static HOME_MUTEX: Mutex<()> = Mutex::new(());

/// Run a test with HOME pointing to a temporary directory so cache
/// operations don't touch the real user's home.
///
/// Uses a unique temp directory per call to avoid races on the same dir.
/// Uses a global mutex to serialize env-var manipulation.
fn with_temp_home<F>(test: F)
where
    F: FnOnce(&PathBuf) + std::panic::UnwindSafe,
{
    let guard = HOME_MUTEX.lock().expect("cache test mutex");

    // Generate a unique suffix so parallel tests don't share the same dir
    let suffix: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!(
        "soroban_cache_test_{}_{}",
        std::process::id(),
        suffix
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create temp home");

    let old_home = std::env::var_os("HOME");
    // SAFETY: serialized by HOME_MUTEX, no other thread reads HOME during this block
    unsafe {
        std::env::set_var("HOME", &tmp);
    }

    // Run the test; catch panics so we can clean up regardless
    let result = std::panic::catch_unwind(|| {
        // Verify the cache dir resolves inside the temp dir
        let home = dirs::home_dir().expect("home dir");
        assert!(
            home.starts_with(&tmp),
            "HOME should point to temp dir: {} vs {}",
            home.display(),
            tmp.display()
        );
        test(&tmp);
    });

    // SAFETY: serialized by HOME_MUTEX, no other thread reads HOME during this block
    if let Some(old) = old_home {
        unsafe {
            std::env::set_var("HOME", old);
        }
    } else {
        unsafe {
            std::env::remove_var("HOME");
        }
    }

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(&tmp);

    // Drop the guard BEFORE resume_unwind to avoid poisoning the mutex
    drop(guard);

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_save_and_load_estimate() {
    with_temp_home(|_tmp| {
        // Save an estimate
        cache::save_estimate(
            "abc123",
            "my_func",
            &["arg1".to_string(), "arg2".to_string()],
            "testnet",
            42,
            1_000_000,
            200_000,
            50_000,
        )
        .expect("save estimate");

        // Load it back
        let loaded = cache::load_estimate(
            "abc123",
            "my_func",
            &["arg1".to_string(), "arg2".to_string()],
        )
        .expect("load estimate")
        .expect("estimate should exist");

        assert_eq!(loaded.wasm_hash, "abc123");
        assert_eq!(loaded.function, "my_func");
        assert_eq!(loaded.network, "testnet");
        assert_eq!(loaded.ledger, 42);
        assert_eq!(loaded.total_stroops, 1_000_000);
        assert_eq!(loaded.cpu_instructions, 200_000);
        assert_eq!(loaded.memory_bytes, 50_000);
    });
}

#[test]
fn test_load_nonexistent_estimate() {
    with_temp_home(|_tmp| {
        let result = cache::load_estimate("nope", "no_func", &[]).expect("load nonexistent");
        assert!(result.is_none(), "nonexistent estimate should return None");
    });
}

#[test]
fn test_different_args_produce_different_cache_keys() {
    with_temp_home(|_tmp| {
        // Save with one set of args
        cache::save_estimate("hash1", "fn1", &["a".to_string()], "testnet", 1, 100, 10, 5)
            .expect("save with args [a]");

        // Save with different args
        cache::save_estimate(
            "hash1",
            "fn1",
            &["b".to_string()],
            "testnet",
            2,
            200,
            20,
            10,
        )
        .expect("save with args [b]");

        // Load with first args → should get ledger 1
        let r1 = cache::load_estimate("hash1", "fn1", &["a".to_string()])
            .expect("load [a]")
            .expect("should exist");
        assert_eq!(r1.ledger, 1);

        // Load with second args → should get ledger 2
        let r2 = cache::load_estimate("hash1", "fn1", &["b".to_string()])
            .expect("load [b]")
            .expect("should exist");
        assert_eq!(r2.ledger, 2);
    });
}

#[test]
fn test_list_cached_estimates_filters_by_network() {
    with_temp_home(|_tmp| {
        // Save estimates for two networks (different functions so they don't collide)
        cache::save_estimate("h1", "f_testnet", &[], "testnet", 1, 100, 10, 5)
            .expect("testnet save");
        cache::save_estimate("h1", "f_mainnet", &[], "mainnet", 2, 200, 20, 10)
            .expect("mainnet save");

        let testnet_estimates = cache::list_cached_estimates("testnet").expect("list testnet");
        assert_eq!(testnet_estimates.len(), 1, "should have 1 testnet estimate");
        assert_eq!(testnet_estimates[0].ledger, 1);

        let mainnet_estimates = cache::list_cached_estimates("mainnet").expect("list mainnet");
        assert_eq!(mainnet_estimates.len(), 1, "should have 1 mainnet estimate");
        assert_eq!(mainnet_estimates[0].ledger, 2);

        // Unknown network → empty
        let futurenet = cache::list_cached_estimates("futurenet").expect("list futurenet");
        assert!(futurenet.is_empty(), "futurenet should have no estimates");
    });
}

#[test]
fn test_find_stale_estimates() {
    with_temp_home(|_tmp| {
        // Save at ledger 5
        cache::save_estimate("h1", "f1", &[], "testnet", 5, 100, 10, 5).expect("save at 5");
        // Save at ledger 10
        cache::save_estimate("h1", "f2", &[], "testnet", 10, 200, 20, 10).expect("save at 10");
        // Save at ledger 15
        cache::save_estimate("h1", "f3", &[], "testnet", 15, 300, 30, 15).expect("save at 15");

        let all = cache::list_cached_estimates("testnet").expect("list all");
        assert_eq!(all.len(), 3, "should have 3 estimates");

        // Current ledger = 12 → stale = ones at 5 and 10
        let stale = cache::find_stale_estimates(&all, 12);
        assert_eq!(stale.len(), 2, "should find 2 stale at ledger 12");
        let stale_names: Vec<&str> = stale.iter().map(|e| e.function.as_str()).collect();
        assert!(stale_names.contains(&"f1"));
        assert!(stale_names.contains(&"f2"));
        assert!(!stale_names.contains(&"f3"));

        // Current ledger = 5 → only the one at 5 is NOT stale
        let stale = cache::find_stale_estimates(&all, 5);
        assert_eq!(stale.len(), 0, "none should be stale at ledger 5");

        // Current ledger = 20 → all are stale
        let stale = cache::find_stale_estimates(&all, 20);
        assert_eq!(stale.len(), 3, "all should be stale at ledger 20");
    });
}

#[test]
fn test_cache_is_empty_initially() {
    with_temp_home(|_tmp| {
        let estimates = cache::list_cached_estimates("testnet").expect("list on empty cache");
        assert!(estimates.is_empty(), "fresh cache should be empty");
    });
}

#[test]
fn test_overwrite_existing_estimate() {
    with_temp_home(|_tmp| {
        // Save at ledger 10
        cache::save_estimate("h1", "f1", &["x".to_string()], "testnet", 10, 100, 10, 5)
            .expect("first save");

        // Overwrite at ledger 20
        cache::save_estimate("h1", "f1", &["x".to_string()], "testnet", 20, 200, 20, 10)
            .expect("overwrite");

        // Load → should get ledger 20
        let loaded = cache::load_estimate("h1", "f1", &["x".to_string()])
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.ledger, 20);
        assert_eq!(loaded.total_stroops, 200);
    });
}

/// The args hash the cache uses for an empty argument list.
fn empty_args_hash() -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::new().finalize())
}

/// Build a `CachedEstimate` fixture for export tests.
fn sample_entry(
    function: &str,
    ledger: u32,
    total_stroops: i64,
    timestamp: &str,
) -> CachedEstimate {
    CachedEstimate {
        wasm_hash: "abc123".to_string(),
        function: function.to_string(),
        args_hash: empty_args_hash(),
        network: "testnet".to_string(),
        ledger,
        total_stroops,
        cpu_instructions: 100,
        memory_bytes: 50,
        timestamp: timestamp.to_string(),
    }
}

/// Write a `CacheExport` to `path` and return the path.
fn write_export(path: &PathBuf, entries: Vec<CachedEstimate>) -> PathBuf {
    let export = CacheExport {
        schema_version: cache::CACHE_EXPORT_SCHEMA_VERSION,
        exported_at: "2026-01-01T00:00:00+00:00".to_string(),
        entries,
    };
    let json = serde_json::to_string_pretty(&export).expect("serialize export");
    std::fs::write(path, json).expect("write export");
    path.clone()
}

/// Path of the on-disk cache file for an empty-args estimate.
fn cache_file_for(wasm_hash: &str, function: &str) -> PathBuf {
    let home = dirs::home_dir().expect("home dir");
    home.join(".soroban-cost-estimator")
        .join("cache")
        .join(cache::cache_filename(
            wasm_hash,
            function,
            &empty_args_hash(),
        ))
}

#[test]
fn test_import_into_empty_cache() {
    with_temp_home(|tmp| {
        let export_path = write_export(
            &tmp.join("export.json"),
            vec![
                sample_entry("f_new1", 10, 100, "2026-01-01T00:00:00+00:00"),
                sample_entry("f_new2", 20, 200, "2026-01-02T00:00:00+00:00"),
            ],
        );

        let summary = cache::import_cache(&export_path, false).expect("import");
        assert_eq!(summary.imported, 2, "both entries are new");
        assert_eq!(summary.skipped, 0);

        let loaded = cache::load_estimate("abc123", "f_new1", &[])
            .expect("load f_new1")
            .expect("f_new1 should exist");
        assert_eq!(loaded.ledger, 10);
        assert_eq!(loaded.total_stroops, 100);

        let loaded2 = cache::load_estimate("abc123", "f_new2", &[])
            .expect("load f_new2")
            .expect("f_new2 should exist");
        assert_eq!(loaded2.ledger, 20);
    });
}

#[test]
fn test_import_merge_keeps_newer_existing_entry() {
    with_temp_home(|tmp| {
        // Local entry stamped "now" (newer than the export's 2020 stamp).
        cache::save_estimate("abc123", "f_merge", &[], "testnet", 100, 999, 1, 1)
            .expect("seed local");

        let entry = sample_entry("f_merge", 50, 111, "2020-01-01T00:00:00+00:00");
        let export_path = write_export(&tmp.join("export.json"), vec![entry]);

        let summary = cache::import_cache(&export_path, false).expect("import");
        assert_eq!(
            summary.imported, 0,
            "newer local entry must not be replaced"
        );
        assert_eq!(summary.skipped, 1);

        let loaded = cache::load_estimate("abc123", "f_merge", &[])
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.ledger, 100, "local newer entry kept");
        assert_eq!(loaded.total_stroops, 999);
    });
}

#[test]
fn test_import_merge_replaces_older_existing_entry() {
    with_temp_home(|tmp| {
        // Seed a local entry, then backdate its timestamp.
        cache::save_estimate("abc123", "f_upd", &[], "testnet", 10, 100, 1, 1).expect("seed local");
        let cache_file = cache_file_for("abc123", "f_upd");
        let mut on_disk: CachedEstimate =
            serde_json::from_str(&std::fs::read_to_string(&cache_file).expect("read local"))
                .expect("parse local");
        on_disk.timestamp = "2020-01-01T00:00:00+00:00".to_string();
        std::fs::write(
            &cache_file,
            serde_json::to_string_pretty(&on_disk).expect("re-serialize"),
        )
        .expect("rewrite local timestamp");

        // Export carries a newer timestamp for the same key.
        let entry = sample_entry("f_upd", 50, 555, "2026-06-01T00:00:00+00:00");
        let export_path = write_export(&tmp.join("export.json"), vec![entry]);

        let summary = cache::import_cache(&export_path, false).expect("import");
        assert_eq!(
            summary.imported, 1,
            "newer imported entry replaces older local"
        );
        assert_eq!(summary.skipped, 0);

        let loaded = cache::load_estimate("abc123", "f_upd", &[])
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.ledger, 50);
        assert_eq!(loaded.total_stroops, 555);
    });
}

#[test]
fn test_import_overwrite_replaces_existing() {
    with_temp_home(|tmp| {
        // Local entry with a NEWER timestamp — overwrite must still replace it.
        cache::save_estimate("abc123", "f_ow", &[], "testnet", 100, 999, 1, 1).expect("seed local");

        let entry = sample_entry("f_ow", 50, 111, "2020-01-01T00:00:00+00:00");
        let export_path = write_export(&tmp.join("export.json"), vec![entry]);

        let summary = cache::import_cache(&export_path, true).expect("import");
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped, 0);

        let loaded = cache::load_estimate("abc123", "f_ow", &[])
            .expect("load")
            .expect("should exist");
        assert_eq!(
            loaded.ledger, 50,
            "overwrite replaces even a newer local entry"
        );
        assert_eq!(loaded.total_stroops, 111);
    });
}

#[test]
fn test_import_rejects_wrong_schema_version() {
    with_temp_home(|tmp| {
        let export = CacheExport {
            schema_version: 999,
            exported_at: "2026-01-01T00:00:00+00:00".to_string(),
            entries: vec![sample_entry("f1", 1, 1, "2026-01-01T00:00:00+00:00")],
        };
        let path = tmp.join("bad_version.json");
        std::fs::write(&path, serde_json::to_string_pretty(&export).expect("ser")).expect("write");

        let err = cache::import_cache(&path, false).expect_err("must reject version");
        let msg = err.to_string();
        assert!(
            msg.contains("schema version") && msg.contains("999"),
            "error should mention the bad version: {msg}"
        );
    });
}

#[test]
fn test_import_rejects_corrupted_json() {
    with_temp_home(|tmp| {
        let path = tmp.join("corrupt.json");
        std::fs::write(&path, "{ this is not json").expect("write corrupt");

        let err = cache::import_cache(&path, false).expect_err("must reject corrupt file");
        let msg = err.to_string();
        assert!(
            msg.contains("corrupted") && msg.contains("corrupt.json"),
            "error should be informative and name the file: {msg}"
        );
    });
}

#[test]
fn test_import_rejects_wrong_shape_json() {
    with_temp_home(|tmp| {
        let path = tmp.join("wrong_shape.json");
        std::fs::write(&path, r#"{"schema_version":1,"entries":"not-an-array"}"#).expect("write");

        let err = cache::import_cache(&path, false).expect_err("must reject wrong shape");
        let msg = err.to_string();
        assert!(
            msg.contains("corrupted"),
            "wrong shape should be reported as corrupted: {msg}"
        );
    });
}

#[test]
fn test_import_missing_file_errors() {
    with_temp_home(|tmp| {
        let path = tmp.join("does_not_exist.json");
        let err = cache::import_cache(&path, false).expect_err("must error on missing file");
        let msg = err.to_string();
        assert!(
            msg.contains("could not read export file"),
            "error should explain the read failure: {msg}"
        );
    });
}
