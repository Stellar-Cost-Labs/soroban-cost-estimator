use std::path::PathBuf;
use std::sync::Mutex;

use soroban_cost_estimator::cache;

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

#[test]
fn test_invalidate_when_wasm_hash_changes() {
    with_temp_home(|tmp| {
        let wasm_path = tmp.join("contract.wasm");
        std::fs::write(&wasm_path, b"v1").expect("write v1");

        let hash_v1 = "hash-v1";
        let hash_v2 = "hash-v2";

        // First observation records the file identity — nothing to drop yet.
        assert!(!cache::invalidate_if_wasm_changed(&wasm_path, hash_v1).expect("first invalidate"));

        // Save an estimate keyed to the v1 build.
        cache::save_estimate(hash_v1, "increment", &[], "testnet", 1, 100, 10, 5).expect("save v1");

        // Recompile: content changes → hash changes.
        std::fs::write(&wasm_path, b"v2").expect("write v2");
        assert!(cache::invalidate_if_wasm_changed(&wasm_path, hash_v2).expect("invalidate v2"));

        // The v1 estimate was dropped.
        assert!(
            cache::load_estimate(hash_v1, "increment", &[])
                .expect("load v1")
                .is_none(),
            "v1 estimates should be removed after a hash change"
        );

        // Re-observing the unchanged file is a no-op.
        assert!(!cache::invalidate_if_wasm_changed(&wasm_path, hash_v2).expect("re-invalidate"));
    });
}

#[test]
fn test_invalidate_when_wasm_mtime_changes() {
    with_temp_home(|tmp| {
        let wasm_path = tmp.join("contract.wasm");
        std::fs::write(&wasm_path, b"same-bytes").expect("write");

        let hash = "hash-same";

        // First observation records the file identity.
        assert!(!cache::invalidate_if_wasm_changed(&wasm_path, hash).expect("first invalidate"));

        // Save an estimate.
        cache::save_estimate(hash, "increment", &[], "testnet", 1, 100, 10, 5).expect("save");

        // Touch the file: content identical, mtime advanced.
        let file = std::fs::File::options()
            .write(true)
            .open(&wasm_path)
            .expect("open for mtime");
        let times = std::fs::FileTimes::new()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10));
        file.set_times(times).expect("set mtime");

        assert!(cache::invalidate_if_wasm_changed(&wasm_path, hash).expect("invalidate on mtime"));

        // The estimate was dropped even though the hash is unchanged.
        assert!(
            cache::load_estimate(hash, "increment", &[])
                .expect("load")
                .is_none(),
            "estimates should be removed after an mtime-only change"
        );
    });
}

#[test]
fn test_remove_cached_estimates_for_wasm() {
    with_temp_home(|_tmp| {
        // Three estimates: two for the same hash, one for a different hash.
        cache::save_estimate("keep-hash", "a", &[], "testnet", 1, 1, 1, 1).expect("save a");
        cache::save_estimate("drop-hash", "b", &[], "testnet", 2, 2, 2, 2).expect("save b");
        cache::save_estimate("drop-hash", "c", &[], "testnet", 3, 3, 3, 3).expect("save c");

        let removed = cache::remove_cached_estimates_for_wasm("drop-hash").expect("remove");
        assert_eq!(removed, 2, "both drop-hash estimates should be removed");

        assert!(
            cache::load_estimate("keep-hash", "a", &[])
                .expect("load keep")
                .is_some()
        );
        assert!(
            cache::load_estimate("drop-hash", "b", &[])
                .expect("load drop b")
                .is_none()
        );
        assert!(
            cache::load_estimate("drop-hash", "c", &[])
                .expect("load drop c")
                .is_none()
        );
    });
}
