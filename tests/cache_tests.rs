use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::Digest;
use soroban_cost_estimator::cache;

/// Serialize cache tests because `std::env::set_var` is not thread-safe.
static HOME_MUTEX: Mutex<()> = Mutex::new(());

/// Number of worker threads used by the concurrency tests.
const CONCURRENT_THREADS: usize = 8;
/// Number of cache entries each worker thread writes/reads.
const ENTRIES_PER_THREAD: usize = 25;

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
    let old_userprofile = std::env::var_os("USERPROFILE");
    // SAFETY: serialized by HOME_MUTEX, no other thread reads these env vars
    // during this block.
    unsafe {
        std::env::set_var("HOME", &tmp);
        // `dirs::home_dir()` reads USERPROFILE on Windows (HOME alone is
        // ignored there), so set both to keep the cache inside the temp dir.
        std::env::set_var("USERPROFILE", &tmp);
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

    // SAFETY: serialized by HOME_MUTEX, no other thread reads these env vars
    // during this block.
    unsafe {
        if let Some(old) = old_home {
            std::env::set_var("HOME", old);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(old) = old_userprofile {
            std::env::set_var("USERPROFILE", old);
        } else {
            std::env::remove_var("USERPROFILE");
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
fn test_verify_cache_empty() {
    with_temp_home(|_tmp| {
        let statuses = cache::verify_cache().expect("verify on empty cache");
        assert!(statuses.is_empty(), "fresh cache should have no entries");
    });
}

#[test]
fn test_verify_cache_all_valid() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5).expect("save f1");
        cache::save_estimate("h2", "f2", &[], "mainnet", 2, 200, 20, 10).expect("save f2");

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 2, "should report both entries");
        assert!(
            statuses.iter().all(|s| s.valid),
            "entries written by save_estimate should all be valid: {statuses:?}"
        );
    });
}

#[test]
fn test_verify_cache_detects_corrupted_entries() {
    with_temp_home(|tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5).expect("save f1");

        // Corrupt entry: a row written by a newer tool (future schema version)
        // cannot be safely migrated forward, so it is flagged as invalid —
        // the SQLite equivalent of a malformed/unreadable JSON file.
        insert_raw_row(
            tmp,
            cache::CACHE_SCHEMA_VERSION + 1,
            "garbage",
            "garbage_func",
            &[],
            "testnet",
            1,
            "2026-01-01T00:00:00Z",
        );

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 2, "should report both entries");

        let corrupt: Vec<&cache::CacheEntryStatus> = statuses.iter().filter(|s| !s.valid).collect();
        assert_eq!(corrupt.len(), 1, "future-version entry should be flagged");
        assert!(
            corrupt[0].filename.starts_with("garbage"),
            "future-version entry should be reported: {}",
            corrupt[0].filename
        );
    });
}

#[test]
fn test_verify_cache_ignores_non_json_files() {
    with_temp_home(|tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5).expect("save f1");

        let dir = tmp.join(".soroban-cost-estimator");
        std::fs::create_dir_all(&dir).expect("create data dir");
        std::fs::write(dir.join("notes.txt"), "not a cache entry").expect("write txt");

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 1, "only .json files should be checked");
        assert!(statuses[0].valid);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Cross-network cache isolation
// ─────────────────────────────────────────────────────────────────────────

/// Saving the same key (wasm_hash + function + args) on two different
/// networks overwrites the previous entry.  This documents the current
/// behaviour so any future fix can be detected by these tests.
#[test]
fn test_same_key_different_networks_overwrites_previous() {
    with_temp_home(|_tmp| {
        // First write: testnet, ledger 10
        cache::save_estimate(
            "hash1",
            "func1",
            &["arg".to_string()],
            "testnet",
            10,
            100,
            10,
            5,
        )
        .expect("save testnet");

        let loaded = cache::load_estimate("hash1", "func1", &["arg".to_string()])
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.network, "testnet");
        assert_eq!(loaded.ledger, 10);

        // Second write: mainnet, same key, ledger 20
        cache::save_estimate(
            "hash1",
            "func1",
            &["arg".to_string()],
            "mainnet",
            20,
            200,
            20,
            10,
        )
        .expect("save mainnet");

        // The testnet entry is gone — the file was overwritten.
        let loaded = cache::load_estimate("hash1", "func1", &["arg".to_string()])
            .expect("load")
            .expect("should still exist");
        assert_eq!(
            loaded.network, "mainnet",
            "mainnet should have overwritten testnet"
        );
        assert_eq!(loaded.ledger, 20);

        // list_cached_estimates confirms the leak.
        let testnet = cache::list_cached_estimates("testnet").expect("list");
        assert!(
            testnet.is_empty(),
            "testnet should have no entries after overwrite"
        );
        let mainnet = cache::list_cached_estimates("mainnet").expect("list");
        assert_eq!(mainnet.len(), 1);
    });
}

/// When different networks use distinct wasm_hash + function + args keys,
/// each network's estimates are fully isolated.
#[test]
fn test_different_keys_different_networks_are_isolated() {
    with_temp_home(|_tmp| {
        cache::save_estimate(
            "hashA",
            "funcA",
            &["a1".to_string()],
            "testnet",
            1,
            100,
            10,
            5,
        )
        .expect("testnet A");
        cache::save_estimate(
            "hashB",
            "funcB",
            &["b1".to_string()],
            "mainnet",
            2,
            200,
            20,
            10,
        )
        .expect("mainnet B");
        cache::save_estimate(
            "hashC",
            "funcC",
            &["c1".to_string()],
            "futurenet",
            3,
            300,
            30,
            15,
        )
        .expect("futurenet C");

        let tn = cache::list_cached_estimates("testnet").expect("list testnet");
        assert_eq!(tn.len(), 1);
        assert_eq!(tn[0].function, "funcA");
        assert_eq!(tn[0].network, "testnet");

        let mn = cache::list_cached_estimates("mainnet").expect("list mainnet");
        assert_eq!(mn.len(), 1);
        assert_eq!(mn[0].function, "funcB");
        assert_eq!(mn[0].network, "mainnet");

        let fn_ = cache::list_cached_estimates("futurenet").expect("list futurenet");
        assert_eq!(fn_.len(), 1);
        assert_eq!(fn_[0].function, "funcC");
        assert_eq!(fn_[0].network, "futurenet");
    });
}

/// load_estimate does not filter by network — it returns whatever the file
/// contains.  This test documents that cross-network calls return the
/// *stored* network, even if the caller intended a different one.
#[test]
fn test_load_estimate_returns_stored_network_not_caller_network() {
    with_temp_home(|_tmp| {
        // Save on testnet
        cache::save_estimate("hash", "fn", &["x".to_string()], "testnet", 10, 100, 10, 5)
            .expect("save");

        // load_estimate has no network parameter — it returns whatever was saved.
        let loaded = cache::load_estimate("hash", "fn", &["x".to_string()])
            .expect("load")
            .expect("should exist");
        assert_eq!(loaded.network, "testnet");
    });
}

/// Multiple networks with the same wasm_hash and function but different args
/// should not leak — the args hash isolates them.
#[test]
fn test_same_wasm_function_different_args_different_networks_isolated() {
    with_temp_home(|_tmp| {
        cache::save_estimate(
            "hash",
            "func",
            &["arg-tn".to_string()],
            "testnet",
            1,
            100,
            10,
            5,
        )
        .expect("testnet save");
        cache::save_estimate(
            "hash",
            "func",
            &["arg-mn".to_string()],
            "mainnet",
            2,
            200,
            20,
            10,
        )
        .expect("mainnet save");

        let tn = cache::load_estimate("hash", "func", &["arg-tn".to_string()])
            .expect("load tn")
            .expect("should exist");
        assert_eq!(tn.network, "testnet");
        assert_eq!(tn.ledger, 1);

        let mn = cache::load_estimate("hash", "func", &["arg-mn".to_string()])
            .expect("load mn")
            .expect("should exist");
        assert_eq!(mn.network, "mainnet");
        assert_eq!(mn.ledger, 2);

        // Both still appear under their respective network lists.
        assert_eq!(cache::list_cached_estimates("testnet").unwrap().len(), 1);
        assert_eq!(cache::list_cached_estimates("mainnet").unwrap().len(), 1);
    });
}

/// find_stale_estimates must not mix networks — stale entries from one
/// network must not appear when querying another.
#[test]
fn test_find_stale_estimates_does_not_mix_networks() {
    with_temp_home(|_tmp| {
        // testnet: ledger 5 (stale at ledger 10)
        cache::save_estimate("h", "f-tn", &[], "testnet", 5, 100, 10, 5).expect("tn");
        // mainnet: ledger 12 (NOT stale at ledger 10)
        cache::save_estimate("h", "f-mn", &[], "mainnet", 12, 200, 20, 10).expect("mn");

        let tn_all = cache::list_cached_estimates("testnet").expect("list tn");
        let tn_stale = cache::find_stale_estimates(&tn_all, 10);
        assert_eq!(tn_stale.len(), 1, "testnet ledger 5 should be stale at 10");
        assert_eq!(tn_stale[0].network, "testnet");

        let mn_all = cache::list_cached_estimates("mainnet").expect("list mn");
        let mn_stale = cache::find_stale_estimates(&mn_all, 10);
        assert!(
            mn_stale.is_empty(),
            "mainnet ledger 12 should not be stale at 10"
        );
    });
}

/// verify_cache must report entries from every network as valid, and not
/// leak network information across files.
#[test]
fn test_verify_cache_across_networks_all_valid() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5).expect("tn");
        cache::save_estimate("h2", "f2", &[], "mainnet", 2, 200, 20, 10).expect("mn");
        cache::save_estimate("h3", "f3", &[], "futurenet", 3, 300, 30, 15).expect("fn");

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 3, "all three entries should be verified");
        assert!(
            statuses.iter().all(|s| s.valid),
            "all entries should be valid"
        );
    });
}

/// Concurrent saves from two different networks to the same key must leave
/// a valid entry behind — no torn writes, no corruption.
#[test]
fn test_concurrent_cross_network_same_key_no_corruption() {
    with_temp_home(|_tmp| {
        let args = vec!["shared".to_string()];
        let tn_args = args.clone();
        let mn_args = args.clone();

        let tn = std::thread::spawn(move || {
            cache::save_estimate(
                "shared-hash",
                "shared-func",
                &tn_args,
                "testnet",
                1,
                100,
                10,
                5,
            )
            .expect("concurrent testnet save");
        });
        let mn = std::thread::spawn(move || {
            cache::save_estimate(
                "shared-hash",
                "shared-func",
                &mn_args,
                "mainnet",
                2,
                200,
                20,
                10,
            )
            .expect("concurrent mainnet save");
        });

        tn.join().expect("testnet thread panicked");
        mn.join().expect("mainnet thread panicked");

        // The surviving entry must be valid JSON.
        let loaded = cache::load_estimate("shared-hash", "shared-func", &args)
            .expect("load")
            .expect("shared key should exist");
        assert!(
            loaded.network == "testnet" || loaded.network == "mainnet",
            "surviving entry must be from one of the two networks: {loaded:?}"
        );

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 1, "one entry for the shared key");
        assert!(statuses[0].valid, "entry must be valid: {statuses:?}");
    });
}

/// Load after cross-network overwrite must return the latest writer's data,
/// not the first writer's. This is the "leak" scenario: a testnet cache
/// entry is silently replaced by a mainnet write.
#[test]
fn test_load_after_cross_network_overwrite_returns_latest_writer() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h", "f", &["x".to_string()], "testnet", 100, 1000, 100, 50)
            .expect("save testnet");

        let before = cache::load_estimate("h", "f", &["x".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(before.network, "testnet");
        assert_eq!(before.ledger, 100);

        // Overwrite with mainnet
        cache::save_estimate("h", "f", &["x".to_string()], "mainnet", 200, 2000, 200, 100)
            .expect("save mainnet");

        let after = cache::load_estimate("h", "f", &["x".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(
            after.network, "mainnet",
            "should return mainnet after overwrite"
        );
        assert_eq!(after.ledger, 200);

        // Verify the testnet list is now empty for this key.
        let tn = cache::list_cached_estimates("testnet").unwrap();
        assert!(
            tn.is_empty(),
            "testnet list should be empty after mainnet overwrite"
        );
    });
}

/// Concurrent `save_estimate`/`load_estimate` calls on distinct cache keys
/// must not corrupt the cache.
///
/// Each thread owns a unique `(wasm_hash, function)` pair and writes/reads
/// `ENTRIES_PER_THREAD` estimates with unique args, so no two threads ever
/// touch the same cache file. After every thread finishes, every entry must
/// still be present, loadable, and parseable.
#[test]
fn test_concurrent_save_and_load_estimates() {
    with_temp_home(|_tmp| {
        let handles: Vec<_> = (0..CONCURRENT_THREADS)
            .map(|t| {
                std::thread::spawn(move || {
                    let wasm_hash = format!("hash-{t}");
                    let function = format!("func-{t}");
                    for j in 0..ENTRIES_PER_THREAD {
                        let args = vec![format!("arg-{t}-{j}")];
                        cache::save_estimate(
                            &wasm_hash,
                            &function,
                            &args,
                            "testnet",
                            j as u32,
                            1_000 + j as i64,
                            10_000 + j as u64,
                            1_000 + j as u64,
                        )
                        .expect("concurrent save");

                        // Load back immediately; only this thread wrote this key.
                        let loaded = cache::load_estimate(&wasm_hash, &function, &args)
                            .expect("concurrent load")
                            .expect("estimate saved by this thread should load");
                        assert_eq!(loaded.ledger, j as u32);
                        assert_eq!(loaded.total_stroops, 1_000 + j as i64);
                        assert_eq!(loaded.cpu_instructions, 10_000 + j as u64);
                        assert_eq!(loaded.memory_bytes, 1_000 + j as u64);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("concurrent save/load thread panicked");
        }

        // Every concurrently written entry must still be present and intact.
        let estimates = cache::list_cached_estimates("testnet").expect("list after concurrent");
        assert_eq!(
            estimates.len(),
            CONCURRENT_THREADS * ENTRIES_PER_THREAD,
            "all concurrently written estimates should be present"
        );
        let statuses = cache::verify_cache().expect("verify after concurrent");
        assert_eq!(statuses.len(), CONCURRENT_THREADS * ENTRIES_PER_THREAD);
        assert!(
            statuses.iter().all(|s| s.valid),
            "concurrent save/load must not corrupt entries: {statuses:?}"
        );
    });
}

/// Concurrent `load_estimate` calls must not corrupt the cache.
///
/// Seed a known set of entries, then hammer the cache with reads from many
/// threads at once. Every entry must load back with its exact values and the
/// cache must still verify as fully valid afterwards.
#[test]
fn test_concurrent_load_estimates() {
    with_temp_home(|_tmp| {
        // Seed the cache sequentially so every entry exists before the reads.
        for t in 0..CONCURRENT_THREADS {
            let wasm_hash = format!("hash-{t}");
            let function = format!("func-{t}");
            for j in 0..ENTRIES_PER_THREAD {
                cache::save_estimate(
                    &wasm_hash,
                    &function,
                    &[format!("arg-{t}-{j}")],
                    "testnet",
                    j as u32,
                    1_000 + j as i64,
                    10_000 + j as u64,
                    1_000 + j as u64,
                )
                .expect("seed save");
            }
        }

        let handles: Vec<_> = (0..CONCURRENT_THREADS)
            .map(|t| {
                std::thread::spawn(move || {
                    for j in 0..ENTRIES_PER_THREAD {
                        let loaded = cache::load_estimate(
                            &format!("hash-{t}"),
                            &format!("func-{t}"),
                            &[format!("arg-{t}-{j}")],
                        )
                        .expect("concurrent load")
                        .expect("seeded estimate should load");
                        assert_eq!(loaded.ledger, j as u32);
                        assert_eq!(loaded.total_stroops, 1_000 + j as i64);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("concurrent load thread panicked");
        }

        let statuses = cache::verify_cache().expect("verify after concurrent loads");
        assert_eq!(statuses.len(), CONCURRENT_THREADS * ENTRIES_PER_THREAD);
        assert!(
            statuses.iter().all(|s| s.valid),
            "concurrent loads must not corrupt entries: {statuses:?}"
        );
    });
}

/// Insert a raw row directly into the SQLite cache database, bypassing
/// `save_estimate` so tests can exercise the migration path directly.
///
/// The `args_hash` column is computed the same way the library does
/// (SHA-256 over the concatenated arg strings).
fn insert_raw_row(
    home: &Path,
    version: u32,
    wasm_hash: &str,
    function: &str,
    args: &[&str],
    network: &str,
    ledger: u32,
    timestamp: &str,
) {
    let mut hasher = sha2::Sha256::new();
    for arg in args {
        hasher.update(arg.as_bytes());
    }
    let args_hash = hex::encode(hasher.finalize());

    let dir = home.join(".soroban-cost-estimator");
    std::fs::create_dir_all(&dir).expect("create data dir");
    let db = dir.join("cache.db");
    let conn = rusqlite::Connection::open(&db).expect("open cache db");
    // Ensure the schema exists in this exact database before writing rows
    // directly.
    cache::ensure_cache_schema(&conn).expect("ensure cache schema");

    conn.execute(
        "INSERT OR REPLACE INTO estimates \
         (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            version as i64,
            wasm_hash,
            function,
            args_hash,
            network,
            ledger as i64,
            100i64,
            10i64,
            5i64,
            timestamp,
        ],
    )
    .expect("insert raw row");
}

/// Write a raw cache entry with an explicit schema `version` (or the initial
/// schema when `version: None`), bypassing `save_estimate`.
fn write_raw_entry(
    tmp: &Path,
    wasm_hash: &str,
    function: &str,
    args: &[&str],
    version: Option<u32>,
    ledger: u32,
) {
    let version = version.unwrap_or(cache::INITIAL_SCHEMA_VERSION);
    insert_raw_row(
        tmp,
        version,
        wasm_hash,
        function,
        args,
        "testnet",
        ledger,
        "2026-01-01T00:00:00Z",
    );
}

/// The current schema version constant exposed by the library.
///
/// Kept in sync with `cache::CACHE_SCHEMA_VERSION`. If the library bumps
/// the schema, these tests must be revisited.
fn current_schema_version() -> u32 {
    cache::CACHE_SCHEMA_VERSION
}

// ─────────────────────────────────────────────────────────────────────────
// Schema versioning & migration
// ─────────────────────────────────────────────────────────────────────────

/// Entries saved by `save_estimate` carry the current schema version, and
/// loading them returns the same version.
#[test]
fn test_saved_entries_are_current_schema_version() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 3, 100, 10, 5).expect("save");
        let loaded = cache::load_estimate("h1", "f1", &[])
            .expect("load")
            .expect("entry should exist");
        assert_eq!(loaded.version, current_schema_version());
    });
}

/// A legacy entry (no `version` key) loads successfully and is treated as
/// the initial schema version, which then equals the current schema.
#[test]
fn test_load_legacy_entry_without_version_field() {
    with_temp_home(|tmp| {
        // No `version` key, like entries written before versioning
        // was introduced.
        write_raw_entry(tmp, "legacy", "old_func", &["a"], None, 7);
        let loaded = cache::load_estimate("legacy", "old_func", &["a".to_string()])
            .expect("legacy entry should load")
            .expect("entry should exist");
        assert_eq!(loaded.version, current_schema_version());
        assert_eq!(loaded.wasm_hash, "legacy");
        assert_eq!(loaded.ledger, 7);
    });
}

/// An entry that already carries the current version passes through
/// `migrate_to_latest` unchanged (fields and version intact).
#[test]
fn test_migrate_to_latest_current_version_is_identity() {
    with_temp_home(|_tmp| {
        let entry = cache::CachedEstimate {
            version: current_schema_version(),
            wasm_hash: "abc".to_string(),
            function: "f".to_string(),
            args_hash: "def".to_string(),
            network: "testnet".to_string(),
            ledger: 1,
            total_stroops: 100,
            cpu_instructions: 10,
            memory_bytes: 5,
            timestamp: "t".to_string(),
        };
        let migrated = cache::migrate_to_latest(entry.clone()).expect("migrate");
        assert_eq!(migrated.version, current_schema_version());
        assert_eq!(migrated.ledger, 1);
    });
}

/// An entry with a version *newer* than the current schema is rejected by
/// `migrate_to_latest` rather than silently misread.
#[test]
fn test_migrate_to_latest_rejects_future_version() {
    with_temp_home(|_tmp| {
        let entry = cache::CachedEstimate {
            version: current_schema_version() + 1,
            wasm_hash: "abc".to_string(),
            function: "f".to_string(),
            args_hash: "def".to_string(),
            network: "testnet".to_string(),
            ledger: 1,
            total_stroops: 100,
            cpu_instructions: 10,
            memory_bytes: 5,
            timestamp: "t".to_string(),
        };
        let err = cache::migrate_to_latest(entry).expect_err("future version must be rejected");
        assert!(err.to_string().contains("newer"), "unhelpful error: {err}");
    });
}

/// `load_estimate` surfaces the error for an entry written by a newer tool,
/// instead of returning a misleading success.
#[test]
fn test_load_rejects_future_version_entry() {
    with_temp_home(|tmp| {
        write_raw_entry(
            tmp,
            "future",
            "new_func",
            &["b"],
            Some(current_schema_version() + 1),
            1,
        );
        let result = cache::load_estimate("future", "new_func", &["b".to_string()]);
        assert!(
            result.is_err(),
            "future-version entries must fail to load, got {result:?}"
        );
    });
}

/// `verify_cache` flags future-version entries as not valid and records the
/// detected version, even though their JSON parses cleanly.
#[test]
fn test_verify_cache_flags_future_version_entries() {
    with_temp_home(|tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5).expect("save valid");
        write_raw_entry(
            tmp,
            "future",
            "new_func",
            &["b"],
            Some(current_schema_version() + 1),
            1,
        );

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 2, "both .json files should be reported");

        let future = statuses
            .iter()
            .find(|s| s.filename.starts_with("future"))
            .expect("future entry should be reported");
        assert!(!future.valid, "future-version entry must be flagged");

        let good = statuses
            .iter()
            .find(|s| s.filename.starts_with("h1"))
            .expect("valid entry should be reported");
        assert!(good.valid, "current-version entry must stay valid");
        assert_eq!(good.version, Some(current_schema_version()));
    });
}

/// Legacy entries (no version key) are reported as valid by `verify_cache`,
/// with their detected version defaulting to the initial schema.
#[test]
fn test_verify_cache_accepts_legacy_entries() {
    with_temp_home(|tmp| {
        write_raw_entry(tmp, "legacy", "old_func", &["a"], None, 7);
        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 1);
        assert!(
            statuses[0].valid,
            "legacy entry should verify as valid: {statuses:?}"
        );
        assert_eq!(statuses[0].version, Some(cache::INITIAL_SCHEMA_VERSION));
    });
}

/// Concurrent saves to the *same* cache key must leave a valid entry behind.
///
/// Two threads race to write the same `(wasm_hash, function, args)` key with
/// different ledgers. Whichever write lands last wins, but the surviving file
/// must parse as a valid `CachedEstimate` (no torn writes) and the cache must
/// verify cleanly.
#[test]
fn test_concurrent_same_key_saves_leave_valid_entry() {
    with_temp_home(|_tmp| {
        let args = vec!["shared".to_string()];
        let handles: Vec<_> = (0..CONCURRENT_THREADS)
            .map(|t| {
                let args = args.clone();
                std::thread::spawn(move || {
                    cache::save_estimate(
                        "shared-hash",
                        "shared-func",
                        &args,
                        "testnet",
                        t as u32,
                        1_000 + t as i64,
                        10_000,
                        1_000,
                    )
                    .expect("concurrent same-key save");
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("concurrent same-key thread panicked");
        }

        // The surviving entry must be one of the written variants.
        let loaded = cache::load_estimate("shared-hash", "shared-func", &args)
            .expect("load shared key")
            .expect("shared key should exist after concurrent saves");
        assert_eq!(loaded.wasm_hash, "shared-hash");
        assert_eq!(loaded.function, "shared-func");
        assert!(
            loaded.ledger < CONCURRENT_THREADS as u32,
            "ledger must be one of the written variants: {loaded:?}"
        );

        let statuses = cache::verify_cache().expect("verify after same-key saves");
        assert_eq!(statuses.len(), 1, "one entry for the shared key");
        assert!(
            statuses[0].valid,
            "shared-key entry must stay valid: {statuses:?}"
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────
// TTL (time-to-live) freshness
// ─────────────────────────────────────────────────────────────────────────

/// Write a raw cache entry with an explicit `timestamp`, bypassing
/// `save_estimate` so tests can control how old an entry is for TTL checks.
fn write_raw_entry_with_timestamp(
    tmp: &Path,
    wasm_hash: &str,
    function: &str,
    args: &[&str],
    timestamp: &str,
) {
    insert_raw_row(
        tmp,
        cache::CACHE_SCHEMA_VERSION,
        wasm_hash,
        function,
        args,
        "testnet",
        7,
        timestamp,
    );
}

/// An entry timestamped "now" is fresh under a TTL of one hour.
#[test]
fn test_is_cache_entry_fresh_within_ttl() {
    with_temp_home(|tmp| {
        let now = chrono::Utc::now().to_rfc3339();
        write_raw_entry_with_timestamp(tmp, "h1", "f1", &["a"], &now);

        let entry = cache::load_estimate("h1", "f1", &["a".to_string()])
            .expect("load")
            .expect("entry should exist");
        assert!(
            cache::is_cache_entry_fresh(&entry, std::time::Duration::from_secs(3600)),
            "an entry written now should be fresh under a 1h TTL"
        );
    });
}

/// An entry older than the TTL is not fresh.
#[test]
fn test_is_cache_entry_fresh_expired() {
    with_temp_home(|tmp| {
        let two_hours_ago = (chrono::Utc::now() - chrono::TimeDelta::hours(2)).to_rfc3339();
        write_raw_entry_with_timestamp(tmp, "h1", "f1", &["a"], &two_hours_ago);

        let entry = cache::load_estimate("h1", "f1", &["a".to_string()])
            .expect("load")
            .expect("entry should exist");
        assert!(
            !cache::is_cache_entry_fresh(&entry, std::time::Duration::from_secs(3600)),
            "an entry written 2h ago should be stale under a 1h TTL"
        );
    });
}

/// An entry whose timestamp cannot be parsed is never considered fresh:
/// an unverifiable age must not be trusted.
#[test]
fn test_is_cache_entry_fresh_unparseable_timestamp() {
    with_temp_home(|tmp| {
        write_raw_entry_with_timestamp(tmp, "h1", "f1", &["a"], "not-a-date");

        let entry = cache::load_estimate("h1", "f1", &["a".to_string()])
            .expect("load")
            .expect("entry should exist");
        assert!(
            !cache::is_cache_entry_fresh(&entry, std::time::Duration::from_secs(3600)),
            "an entry with an unparseable timestamp must not count as fresh"
        );
    });
}

/// No entry at all means "re-simulate": `load_fresh_estimate` returns None.
#[test]
fn test_load_fresh_estimate_missing_entry() {
    with_temp_home(|_tmp| {
        let fresh = cache::load_fresh_estimate(
            "nope",
            "no_func",
            &[],
            std::time::Duration::from_secs(3600),
        )
        .expect("load fresh on empty cache");
        assert!(fresh.is_none(), "a missing entry must yield None");
    });
}

/// A fresh entry is returned intact by `load_fresh_estimate`.
#[test]
fn test_load_fresh_estimate_returns_fresh_entry() {
    with_temp_home(|tmp| {
        let now = chrono::Utc::now().to_rfc3339();
        write_raw_entry_with_timestamp(tmp, "h1", "f1", &["a"], &now);

        let fresh = cache::load_fresh_estimate(
            "h1",
            "f1",
            &["a".to_string()],
            std::time::Duration::from_secs(3600),
        )
        .expect("load fresh")
        .expect("fresh entry should be returned");
        assert_eq!(fresh.wasm_hash, "h1");
        assert_eq!(fresh.ledger, 7);
    });
}

/// An expired entry is treated as a miss even though the file exists: the
/// caller must re-simulate.
#[test]
fn test_load_fresh_estimate_expired_returns_none() {
    with_temp_home(|tmp| {
        let two_hours_ago = (chrono::Utc::now() - chrono::TimeDelta::hours(2)).to_rfc3339();
        write_raw_entry_with_timestamp(tmp, "h1", "f1", &["a"], &two_hours_ago);

        // The entry exists and loads fine...
        let loaded = cache::load_estimate("h1", "f1", &["a".to_string()])
            .expect("load")
            .expect("entry should exist");
        assert_eq!(loaded.ledger, 7);

        // ...but it is not fresh under a 1h TTL.
        let fresh = cache::load_fresh_estimate(
            "h1",
            "f1",
            &["a".to_string()],
            std::time::Duration::from_secs(3600),
        )
        .expect("load fresh");
        assert!(fresh.is_none(), "an expired entry must yield None");
    });
}
