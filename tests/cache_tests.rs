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
        // The library prefers $HOME on Unix and $USERPROFILE on Windows when
        // resolving the data dir, so set both to keep the cache inside the
        // temp dir on every platform.
        std::env::set_var("USERPROFILE", &tmp);
    }

    // Run the test; catch panics so we can clean up regardless
    let result = std::panic::catch_unwind(|| {
        // Verify the cache dir resolves inside the temp dir
        let data_dir = soroban_cost_estimator::paths::data_dir().expect("data dir");
        assert!(
            data_dir.starts_with(&tmp),
            "data dir should point to temp dir: {} vs {}",
            data_dir.display(),
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
            None,
            true,
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
        cache::save_estimate(
            "hash1",
            "fn1",
            &["a".to_string()],
            "testnet",
            1,
            100,
            10,
            5,
            None,
            true,
        )
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
            None,
            true,
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
        cache::save_estimate("h1", "f_testnet", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("testnet save");
        cache::save_estimate(
            "h1",
            "f_mainnet",
            &[],
            "mainnet",
            2,
            200,
            20,
            10,
            None,
            true,
        )
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
        cache::save_estimate("h1", "f1", &[], "testnet", 5, 100, 10, 5, None, true)
            .expect("save at 5");
        // Save at ledger 10
        cache::save_estimate("h1", "f2", &[], "testnet", 10, 200, 20, 10, None, true)
            .expect("save at 10");
        // Save at ledger 15
        cache::save_estimate("h1", "f3", &[], "testnet", 15, 300, 30, 15, None, true)
            .expect("save at 15");

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
        cache::save_estimate(
            "h1",
            "f1",
            &["x".to_string()],
            "testnet",
            10,
            100,
            10,
            5,
            None,
            true,
        )
        .expect("first save");

        // Overwrite at ledger 20
        cache::save_estimate(
            "h1",
            "f1",
            &["x".to_string()],
            "testnet",
            20,
            200,
            20,
            10,
            None,
            true,
        )
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
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save f1");
        cache::save_estimate("h2", "f2", &[], "mainnet", 2, 200, 20, 10, None, true)
            .expect("save f2");

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
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save f1");

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
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save f1");

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
            None,
            true,
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
            None,
            true,
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
            None,
            true,
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
            None,
            true,
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
            None,
            true,
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
        cache::save_estimate(
            "hash",
            "fn",
            &["x".to_string()],
            "testnet",
            10,
            100,
            10,
            5,
            None,
            true,
        )
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
            None,
            true,
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
            None,
            true,
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
        cache::save_estimate("h", "f-tn", &[], "testnet", 5, 100, 10, 5, None, true).expect("tn");
        // mainnet: ledger 12 (NOT stale at ledger 10)
        cache::save_estimate("h", "f-mn", &[], "mainnet", 12, 200, 20, 10, None, true).expect("mn");

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
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true).expect("tn");
        cache::save_estimate("h2", "f2", &[], "mainnet", 2, 200, 20, 10, None, true).expect("mn");
        cache::save_estimate("h3", "f3", &[], "futurenet", 3, 300, 30, 15, None, true).expect("fn");

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
                None,
                true,
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
                None,
                true,
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
        cache::save_estimate(
            "h",
            "f",
            &["x".to_string()],
            "testnet",
            100,
            1000,
            100,
            50,
            None,
            true,
        )
        .expect("save testnet");

        let before = cache::load_estimate("h", "f", &["x".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(before.network, "testnet");
        assert_eq!(before.ledger, 100);

        // Overwrite with mainnet
        cache::save_estimate(
            "h",
            "f",
            &["x".to_string()],
            "mainnet",
            200,
            2000,
            200,
            100,
            None,
            true,
        )
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
                            None,
                            true,
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
                    None,
                    true,
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
// clear_cache (issue #24)
// ─────────────────────────────────────────────────────────────────────────

/// `clear_cache` deletes every entry recorded for the requested network and
/// returns the number of rows removed, leaving other networks untouched.
#[test]
fn test_clear_cache_removes_requested_network_only() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save testnet f1");
        cache::save_estimate("h2", "f2", &[], "testnet", 2, 200, 20, 10, None, true)
            .expect("save testnet f2");
        cache::save_estimate("h3", "f3", &[], "mainnet", 3, 300, 30, 15, None, true)
            .expect("save mainnet f3");

        let removed = cache::clear_cache("testnet").expect("clear testnet");
        assert_eq!(
            removed, 2,
            "clearing testnet should delete its two entries, got {removed}"
        );

        let testnet = cache::list_cached_estimates("testnet").expect("list testnet");
        assert!(testnet.is_empty(), "testnet should be empty after clearing");
        let mainnet = cache::list_cached_estimates("mainnet").expect("list mainnet");
        assert_eq!(mainnet.len(), 1, "mainnet entries must be untouched");
        assert_eq!(mainnet[0].function, "f3");
        assert_eq!(mainnet[0].network, "mainnet");

        // A second clear on the already-empty network deletes nothing.
        let removed = cache::clear_cache("testnet").expect("clear testnet again");
        assert_eq!(removed, 0, "second clear should delete nothing");
    });
}

/// Clearing a network with no entries (or a brand-new cache) is a no-op that
/// reports zero and does not error.
#[test]
fn test_clear_cache_on_empty_cache_returns_zero() {
    with_temp_home(|_tmp| {
        let removed = cache::clear_cache("futurenet").expect("clear empty cache");
        assert_eq!(removed, 0, "an empty cache should report zero cleared");
    });
}

/// Clearing one network must never leak into a different network's entries.
#[test]
fn test_clear_cache_never_touches_other_networks() {
    with_temp_home(|_tmp| {
        cache::save_estimate("hA", "fA", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save testnet");
        cache::save_estimate("hB", "fB", &[], "mainnet", 2, 200, 20, 10, None, true)
            .expect("save mainnet");
        cache::save_estimate("hC", "fC", &[], "futurenet", 3, 300, 30, 15, None, true)
            .expect("save futurenet");

        let removed = cache::clear_cache("mainnet").expect("clear mainnet");
        assert_eq!(removed, 1, "only the mainnet entry should be deleted");

        assert_eq!(cache::list_cached_estimates("testnet").unwrap().len(), 1);
        assert_eq!(cache::list_cached_estimates("mainnet").unwrap().len(), 0);
        assert_eq!(cache::list_cached_estimates("futurenet").unwrap().len(), 1);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Schema versioning & migration
// ─────────────────────────────────────────────────────────────────────────

/// Entries saved by `save_estimate` carry the current schema version, and
/// loading them returns the same version.
#[test]
fn test_saved_entries_are_current_schema_version() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 3, 100, 10, 5, None, true).expect("save");
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
            duration_ms: Some(42),
            success: true,
            io: None,
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
            duration_ms: None,
            success: true,
            io: None,
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
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save valid");
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
                        None,
                        true,
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

/// The export envelope carries a schema version, an export timestamp, and
/// the estimate records — everything a backup needs to be self-describing.
#[test]
fn test_export_cache_envelope_shape() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true).expect("save");

        let export = cache::export_cache(None).expect("export");
        assert_eq!(
            export.schema_version,
            cache::CACHE_EXPORT_SCHEMA_VERSION,
            "envelope should stamp the export schema version"
        );
        assert!(
            chrono::DateTime::parse_from_rfc3339(&export.exported_at).is_ok(),
            "exported_at should be RFC-3339: {}",
            export.exported_at
        );
        assert!(export.network.is_none(), "unfiltered export has no network");
        assert_eq!(export.estimates.len(), 1, "should export the seeded entry");
        assert_eq!(export.estimates[0].function, "f1");
    });
}

/// A network filter restricts the export to that network's estimates.
#[test]
fn test_export_cache_filters_by_network() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f_testnet", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("testnet save");
        cache::save_estimate(
            "h1",
            "f_mainnet",
            &[],
            "mainnet",
            2,
            200,
            20,
            10,
            None,
            true,
        )
        .expect("mainnet save");

        let testnet = cache::export_cache(Some("testnet")).expect("export testnet");
        assert_eq!(testnet.network, Some("testnet".to_string()));
        assert_eq!(testnet.estimates.len(), 1, "should export only testnet");
        assert_eq!(testnet.estimates[0].function, "f_testnet");

        let all = cache::export_cache(None).expect("export all");
        assert!(all.network.is_none());
        assert_eq!(
            all.estimates.len(),
            2,
            "unfiltered export should carry both"
        );

        let empty = cache::export_cache(Some("futurenet")).expect("export futurenet");
        assert_eq!(empty.network, Some("futurenet".to_string()));
        assert!(
            empty.estimates.is_empty(),
            "unknown network exports nothing"
        );
    });
}

/// The envelope round-trips through JSON — the shape backups are stored in.
#[test]
fn test_export_cache_json_round_trip() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true).expect("save");

        let export = cache::export_cache(Some("testnet")).expect("export");
        let json = serde_json::to_string_pretty(&export).expect("serialize");
        let parsed: cache::CacheExport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.schema_version, cache::CACHE_EXPORT_SCHEMA_VERSION);
        assert_eq!(parsed.network, Some("testnet".to_string()));
        assert_eq!(parsed.estimates.len(), 1);
        assert_eq!(parsed.estimates[0].total_stroops, 100);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────
// SQLite storage backend: indexes & legacy JSON migration (#334)
// ─────────────────────────────────────────────────────────────────────────

/// The SHA-256 hex of the concatenated args, matching the library's cache
/// key derivation.
fn args_hash_of(args: &[&str]) -> String {
    let mut hasher = sha2::Sha256::new();
    for arg in args {
        hasher.update(arg.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Path of the data directory under a temp HOME.
fn data_dir(tmp: &Path) -> PathBuf {
    tmp.join(".soroban-cost-estimator")
}

/// Write a legacy (pre-SQLite) cache entry JSON file into
/// `~/.soroban-cost-estimator/cache/`.
fn write_legacy_cache_file(tmp: &Path, name: &str, contents: &str) {
    let dir = data_dir(tmp).join("cache");
    std::fs::create_dir_all(&dir).expect("create legacy cache dir");
    std::fs::write(dir.join(name), contents).expect("write legacy cache file");
}

/// SQLite index names present on the `estimates` table.
fn estimate_index_names(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'estimates'")
        .expect("prepare index query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query indexes")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect indexes")
}

#[test]
fn test_sqlite_schema_creates_lookup_indexes() {
    with_temp_home(|tmp| {
        let dir = data_dir(tmp);
        std::fs::create_dir_all(&dir).expect("create data dir");
        let conn = rusqlite::Connection::open(dir.join("cache.db")).expect("open cache db");
        cache::ensure_cache_schema(&conn).expect("ensure schema");

        let names = estimate_index_names(&conn);
        for expected in [
            "idx_estimates_lookup",
            "idx_estimates_network_created",
            "idx_estimates_key_net_created",
            "idx_estimates_last_accessed",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "expected index {expected} on estimates; found {names:?}"
            );
        }
    });
}

#[test]
fn test_legacy_json_cache_is_imported_transparently() {
    with_temp_home(|tmp| {
        let legacy = format!(
            r#"{{
  "wasm_hash": "abc",
  "function": "f",
  "args_hash": "{}",
  "network": "testnet",
  "ledger": 7,
  "total_stroops": 1234,
  "cpu_instructions": 10,
  "memory_bytes": 5,
  "timestamp": "2026-01-01T00:00:00Z"
}}"#,
            args_hash_of(&[])
        );
        write_legacy_cache_file(tmp, "abc-f.json", &legacy);

        // No explicit migration call: opening the cache does it transparently.
        let loaded = cache::load_estimate("abc", "f", &[])
            .expect("load migrated entry")
            .expect("legacy entry should have been imported");
        assert_eq!(loaded.total_stroops, 1234);
        assert_eq!(loaded.ledger, 7);
        assert_eq!(loaded.version, cache::CACHE_SCHEMA_VERSION);
        assert_eq!(loaded.duration_ms, None);
        assert!(loaded.success);

        // The legacy directory is retired once fully imported.
        assert!(
            !data_dir(tmp).join("cache").exists(),
            "legacy JSON cache dir should be removed after a full import"
        );
    });
}

#[test]
fn test_legacy_json_cache_keeps_unparseable_files() {
    with_temp_home(|tmp| {
        let valid = format!(
            r#"{{"wasm_hash":"abc","function":"f","args_hash":"{}","network":"testnet","ledger":1,"total_stroops":1,"cpu_instructions":1,"memory_bytes":1,"timestamp":"2026-01-01T00:00:00Z"}}"#,
            args_hash_of(&[])
        );
        write_legacy_cache_file(tmp, "good.json", &valid);
        write_legacy_cache_file(tmp, "bad.json", "{ not valid json");

        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert_eq!(estimates.len(), 1, "one valid entry should be imported");

        let legacy_dir = data_dir(tmp).join("cache");
        assert!(
            legacy_dir.join("bad.json.rejected").exists(),
            "unparseable files must be kept under a .rejected extension"
        );
        assert!(
            !legacy_dir.join("bad.json").exists(),
            "the original unparseable file should no longer be retried"
        );
    });
}

#[test]
fn test_legacy_json_entry_from_future_version_is_kept() {
    with_temp_home(|tmp| {
        let future = format!(
            r#"{{"schema_version":{},"wasm_hash":"abc","function":"f","args_hash":"{}","network":"testnet","ledger":1,"total_stroops":1,"cpu_instructions":1,"memory_bytes":1,"timestamp":"2026-01-01T00:00:00Z"}}"#,
            cache::CACHE_SCHEMA_VERSION + 5,
            args_hash_of(&[])
        );
        write_legacy_cache_file(tmp, "future.json", &future);

        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert!(estimates.is_empty(), "future entries must not be imported");
        assert!(
            data_dir(tmp).join("cache").join("future.json").exists(),
            "a future-schema legacy file must be preserved, not deleted"
        );
    });
}

#[test]
fn test_pre_v3_database_gains_last_accessed_column() {
    with_temp_home(|tmp| {
        let dir = data_dir(tmp);
        std::fs::create_dir_all(&dir).expect("create data dir");
        let db = dir.join("cache.db");

        // Recreate the exact pre-v3 table shape: no `last_accessed` column
        // and none of the secondary indexes.
        {
            let conn = rusqlite::Connection::open(&db).expect("open cache db");
            conn.execute_batch(
                "CREATE TABLE estimates (\n                     version INTEGER NOT NULL,\n                     wasm_hash TEXT NOT NULL,\n                     function TEXT NOT NULL,\n                     args_hash TEXT NOT NULL,\n                     network TEXT NOT NULL,\n                     ledger INTEGER NOT NULL,\n                     total_stroops INTEGER NOT NULL,\n                     cpu_instructions INTEGER NOT NULL,\n                     memory_bytes INTEGER NOT NULL,\n                     timestamp TEXT NOT NULL,\n                     duration_ms INTEGER,\n                     success INTEGER NOT NULL DEFAULT 1,\n                     PRIMARY KEY (wasm_hash, function, args_hash)\n                 );",
            )
            .expect("create legacy table");
            conn.execute(
                "INSERT INTO estimates (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp) \
                 VALUES (2, 'h', 'f', ?1, 'testnet', 1, 100, 10, 5, '2026-01-01T00:00:00Z')",
                [args_hash_of(&[])],
            )
            .expect("insert legacy row");
        }

        // Opening through the library migrates the table in place.
        let loaded = cache::load_estimate("h", "f", &[])
            .expect("load")
            .expect("row should survive the table migration");
        assert_eq!(loaded.version, cache::CACHE_SCHEMA_VERSION);
        assert_eq!(loaded.total_stroops, 100);

        let conn = rusqlite::Connection::open(&db).expect("reopen cache db");
        let has_column = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(estimates)")
                .expect("pragma");
            let names: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .expect("query columns")
                .collect::<Result<_, _>>()
                .expect("collect columns");
            names.iter().any(|n| n == "last_accessed")
        };
        assert!(has_column, "pre-v3 cache should gain last_accessed");
        assert!(
            !estimate_index_names(&conn).is_empty(),
            "pre-v3 cache should gain the lookup indexes"
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Schema versioning & migration (#341)
// ─────────────────────────────────────────────────────────────────────────

/// An *unversioned* legacy entry (v0) is migrated to the current schema and
/// gets the conservative defaults for fields it predates.
#[test]
fn test_v0_unversioned_entry_migrates_to_current_schema() {
    with_temp_home(|tmp| {
        write_raw_entry(tmp, "legacy0", "old_func", &["a"], Some(0), 3);

        let loaded = cache::load_estimate("legacy0", "old_func", &["a".to_string()])
            .expect("v0 entry should load")
            .expect("entry should exist");
        assert_eq!(loaded.version, cache::CACHE_SCHEMA_VERSION);
        assert_eq!(loaded.ledger, 3);
        assert_eq!(loaded.duration_ms, None, "v0 has no duration data");
        assert!(loaded.success, "v0 entries were always successful");
    });
}

/// A v2 entry moves to v3 without losing the fields v2 introduced.
#[test]
fn test_v2_entry_migrates_to_v3_preserving_fields() {
    let entry = cache::CachedEstimate {
        version: cache::DURATION_SCHEMA_VERSION,
        wasm_hash: "abc".to_string(),
        function: "f".to_string(),
        args_hash: "def".to_string(),
        network: "testnet".to_string(),
        ledger: 9,
        total_stroops: 4_200,
        cpu_instructions: 111,
        memory_bytes: 22,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        duration_ms: Some(1_234),
        success: false,
        io: None,
    };
    let migrated = cache::migrate_to_latest(entry).expect("migrate v2 to v3");
    assert_eq!(migrated.version, cache::CACHE_SCHEMA_VERSION);
    assert_eq!(migrated.duration_ms, Some(1_234));
    assert!(!migrated.success, "a recorded failure must be preserved");
    assert_eq!(migrated.total_stroops, 4_200);
}

/// Serialized entries carry the `schema_version` key, and the pre-rename
/// `version` key is still accepted on read.
#[test]
fn test_schema_version_serialization_key() {
    let entry = cache::CachedEstimate {
        version: cache::CACHE_SCHEMA_VERSION,
        wasm_hash: "abc".to_string(),
        function: "f".to_string(),
        args_hash: "def".to_string(),
        network: "testnet".to_string(),
        ledger: 1,
        total_stroops: 1,
        cpu_instructions: 1,
        memory_bytes: 1,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        duration_ms: None,
        success: true,
        io: None,
    };
    let json = serde_json::to_string(&entry).expect("serialize");
    assert!(
        json.contains("\"schema_version\""),
        "serialized cache entries must carry schema_version: {json}"
    );

    let with_legacy_key = json.replace("\"schema_version\"", "\"version\"");
    let parsed: cache::CachedEstimate =
        serde_json::from_str(&with_legacy_key).expect("legacy `version` key must deserialize");
    assert_eq!(parsed.version, cache::CACHE_SCHEMA_VERSION);
}

/// An entry with no `version`/`schema_version` key at all (the true v0 JSON
/// shape) deserializes to the initial schema version and migrates forward.
#[test]
fn test_unversioned_json_deserializes_and_migrates() {
    let json = r#"{
        "wasm_hash": "abc",
        "function": "f",
        "args_hash": "def",
        "network": "testnet",
        "ledger": 5,
        "total_stroops": 10,
        "cpu_instructions": 1,
        "memory_bytes": 1,
        "timestamp": "2026-01-01T00:00:00Z"
    }"#;
    let parsed: cache::CachedEstimate = serde_json::from_str(json).expect("v0 JSON");
    assert_eq!(parsed.version, cache::INITIAL_SCHEMA_VERSION);

    let migrated = cache::migrate_to_latest(parsed).expect("migrate");
    assert_eq!(migrated.version, cache::CACHE_SCHEMA_VERSION);
}

/// The upgrade hint names the entry's version so the user knows what to do.
#[test]
fn test_future_schema_error_names_version_and_remedy() {
    let entry = cache::CachedEstimate {
        version: cache::CACHE_SCHEMA_VERSION + 1,
        wasm_hash: "abc".to_string(),
        function: "f".to_string(),
        args_hash: "def".to_string(),
        network: "testnet".to_string(),
        ledger: 1,
        total_stroops: 1,
        cpu_instructions: 1,
        memory_bytes: 1,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        duration_ms: None,
        success: true,
        io: None,
    };
    let err = cache::migrate_to_latest(entry).expect_err("future version must be rejected");
    let message = err.to_string();
    assert!(message.contains("newer"), "{message}");
    assert!(
        message.contains("upgrade"),
        "should include an upgrade hint: {message}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// LRU eviction (#333)
// ─────────────────────────────────────────────────────────────────────────

/// [`cache::CacheLimits`] with only an entry quota (byte quota disabled).
fn entry_limits(max_entries: usize) -> cache::CacheLimits {
    cache::CacheLimits {
        max_bytes: 0,
        max_entries,
    }
}

/// Save one estimate with explicit limits (keeps eviction tests independent
/// of the process-wide CLI configuration).
fn save_with(
    limits: cache::CacheLimits,
    wasm_hash: &str,
    function: &str,
    ledger: u32,
) -> soroban_cost_estimator::error::AppResult<()> {
    cache::save_estimate_with_limits(
        wasm_hash,
        function,
        &[],
        "testnet",
        ledger,
        1_000,
        100,
        50,
        None,
        None,
        true,
        limits,
    )
}

#[test]
fn test_eviction_by_entry_quota_keeps_cache_bounded() {
    with_temp_home(|_tmp| {
        let limits = entry_limits(5);
        for i in 0..10 {
            save_with(limits, "h", &format!("f{i}"), i).expect("save");
        }

        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert!(
            estimates.len() <= 5,
            "entry quota must be respected, got {} entries",
            estimates.len()
        );
        assert!(!estimates.is_empty(), "eviction must not empty the cache");
        assert!(
            estimates.iter().any(|e| e.function == "f9"),
            "the newest entry must survive eviction: {estimates:?}"
        );
        assert!(
            !estimates.iter().any(|e| e.function == "f0"),
            "the oldest entry must be evicted first: {estimates:?}"
        );
    });
}

#[test]
fn test_eviction_protects_most_recently_accessed_entry() {
    with_temp_home(|_tmp| {
        let limits = entry_limits(3);
        let pause = || std::thread::sleep(std::time::Duration::from_millis(2));

        save_with(limits, "h", "f0", 0).expect("save f0");
        pause();
        save_with(limits, "h", "f1", 1).expect("save f1");
        pause();
        save_with(limits, "h", "f2", 2).expect("save f2");
        pause();

        // Reading f0 makes it the most recently accessed entry even though
        // it is the oldest by creation time.
        cache::load_estimate("h", "f0", &[])
            .expect("load f0")
            .expect("f0 exists");
        pause();

        // Pushing past the quota evicts f1 and f2 (LRU order), not f0.
        save_with(limits, "h", "f3", 3).expect("save f3");

        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert!(
            estimates.iter().any(|e| e.function == "f0"),
            "recently accessed f0 must survive: {estimates:?}"
        );
        assert!(
            estimates.iter().any(|e| e.function == "f3"),
            "the newest entry must survive: {estimates:?}"
        );
        assert!(
            !estimates.iter().any(|e| e.function == "f1"),
            "the least-recently-accessed entry must be evicted: {estimates:?}"
        );
    });
}

#[test]
fn test_eviction_by_byte_quota_stays_under_limit() {
    with_temp_home(|_tmp| {
        // A quota comfortably above SQLite's structural floor (page 1 plus a
        // root page per table/index) but well below the size of 600 rows.
        let max_bytes = 96 * 1024;
        let limits = cache::CacheLimits {
            max_bytes,
            max_entries: 0,
        };
        const SAVED: u32 = 600;

        for i in 0..SAVED {
            save_with(limits, &format!("hash-{i:04}"), "f", i).expect("save");
        }

        let stats = cache::cache_stats().expect("stats");
        assert!(
            stats.total_entries < SAVED as usize,
            "byte quota should have evicted entries, kept {}",
            stats.total_entries
        );
        assert!(
            stats.live_bytes <= max_bytes,
            "live cache size {} must stay under the {max_bytes} byte quota",
            stats.live_bytes,
        );
    });
}

/// A byte quota smaller than the database's structural floor must not wipe
/// the cache: eviction stops once deleting rows stops reducing the size.
#[test]
fn test_unattainable_byte_quota_does_not_empty_cache() {
    with_temp_home(|_tmp| {
        // 1 byte is below any SQLite database's floor.
        let limits = cache::CacheLimits {
            max_bytes: 1,
            max_entries: 0,
        };
        for i in 0..20 {
            save_with(limits, &format!("hash-{i:04}"), "f", i).expect("save");
        }

        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert!(
            !estimates.is_empty(),
            "an unattainable byte quota must not evict every entry"
        );
    });
}

#[test]
fn test_unbounded_limits_disable_eviction() {
    with_temp_home(|_tmp| {
        for i in 0..50 {
            save_with(cache::CacheLimits::UNBOUNDED, "h", &format!("f{i}"), i).expect("save");
        }
        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert_eq!(estimates.len(), 50, "unbounded limits must never evict");
    });
}

#[test]
fn test_evict_lru_is_noop_under_default_limits() {
    with_temp_home(|_tmp| {
        for i in 0..5 {
            cache::save_estimate(
                "h",
                &format!("f{i}"),
                &[],
                "testnet",
                i,
                100,
                10,
                5,
                None,
                true,
            )
            .expect("save");
        }
        let evicted = cache::evict_lru().expect("prune");
        assert_eq!(evicted, 0, "a small cache is within the default quota");
    });
}

#[test]
fn test_concurrent_saves_respect_entry_quota() {
    with_temp_home(|_tmp| {
        let limits = entry_limits(10);
        let handles: Vec<_> = (0..4)
            .map(|t| {
                std::thread::spawn(move || {
                    for j in 0..20 {
                        save_with(limits, &format!("hash-{t}"), &format!("f{j}"), j)
                            .expect("concurrent save");
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("concurrent save thread panicked");
        }

        let estimates = cache::list_cached_estimates("testnet").expect("list");
        assert!(
            estimates.len() <= 10,
            "quota must hold under concurrent writers, got {}",
            estimates.len()
        );
        let statuses = cache::verify_cache().expect("verify after concurrent eviction");
        assert!(
            statuses.iter().all(|s| s.valid),
            "concurrent eviction must not corrupt entries: {statuses:?}"
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────
// cache_stats (issue #281)
// ─────────────────────────────────────────────────────────────────────────

/// `cache_stats` on a fresh cache reports zero entries, no timestamps, an
/// empty per-network breakdown, and no disk error.
#[test]
fn test_cache_stats_empty_cache() {
    with_temp_home(|_tmp| {
        let stats = cache::cache_stats().expect("stats on empty cache");
        assert_eq!(stats.total_entries, 0, "fresh cache should have 0 entries");
        assert!(
            stats.oldest_entry.is_none(),
            "no oldest entry on an empty cache"
        );
        assert!(
            stats.newest_entry.is_none(),
            "no newest entry on an empty cache"
        );
        assert!(
            stats.per_network.is_empty(),
            "empty cache should have no per-network breakdown"
        );
    });
}

/// `cache_stats` aggregates every cached estimate across networks: total
/// count, oldest/newest timestamps, per-network breakdown, and disk usage.
#[test]
fn test_cache_stats_reports_populated_cache() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5, None, true)
            .expect("save testnet f1");
        cache::save_estimate("h2", "f2", &[], "mainnet", 2, 200, 20, 10, None, true)
            .expect("save mainnet f2");

        let stats = cache::cache_stats().expect("stats on populated cache");
        assert_eq!(stats.total_entries, 2, "both entries should be counted");
        assert!(stats.disk_bytes > 0, "the SQLite file should occupy space");
        assert!(
            stats.oldest_entry.is_some(),
            "an oldest timestamp should be reported"
        );
        assert!(
            stats.newest_entry.is_some(),
            "a newest timestamp should be reported"
        );

        let count = |network: &str| {
            stats
                .per_network
                .iter()
                .find(|(name, _)| name == network)
                .map(|(_, count)| *count)
        };
        assert_eq!(
            count("mainnet"),
            Some(1),
            "mainnet breakdown should count 1"
        );
        assert_eq!(
            count("testnet"),
            Some(1),
            "testnet breakdown should count 1"
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────
// query_cache and CacheFilter tests (issue #335)
// ─────────────────────────────────────────────────────────────────────────

fn seed_query_test_entry(
    home: &Path,
    wasm_hash: &str,
    function: &str,
    args: &[&str],
    network: &str,
    total_stroops: i64,
    cpu_instructions: u64,
    timestamp: &str,
) {
    let dir = home.join(".soroban-cost-estimator");
    std::fs::create_dir_all(&dir).expect("create data dir");
    let db = dir.join("cache.db");
    let conn = rusqlite::Connection::open(&db).expect("open cache db");
    cache::ensure_cache_schema(&conn).expect("ensure cache schema");

    let mut hasher = sha2::Sha256::new();
    for a in args {
        hasher.update(a.as_bytes());
    }
    let args_hash = hex::encode(hasher.finalize());

    conn.execute(
        "INSERT OR REPLACE INTO estimates \
         (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp, duration_ms, success) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            cache::CACHE_SCHEMA_VERSION as i64,
            wasm_hash,
            function,
            args_hash,
            network,
            100i64,
            total_stroops,
            cpu_instructions as i64,
            1024i64,
            timestamp,
            Some(50i64),
            1i64,
        ],
    )
    .expect("insert test estimate");
}

#[test]
fn test_query_cache_wasm_hash_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111111111111111111111111111111111111111111111111111111111111111",
            "func_a",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "2222222222222222222222222222222222222222222222222222222222222222",
            "func_b",
            &[],
            "testnet",
            200_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );

        // Exact match
        let filter = cache::CacheFilter {
            wasm_hash: Some(
                "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            ),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].function, "func_a");

        // Case-insensitive prefix match
        let filter_prefix = cache::CacheFilter {
            wasm_hash: Some("2222".to_string()),
            ..Default::default()
        };
        let res_prefix = cache::query_cache(&filter_prefix).expect("query");
        assert_eq!(res_prefix.len(), 1);
        assert_eq!(res_prefix[0].function, "func_b");

        // Non-matching hash returns empty
        let filter_none = cache::CacheFilter {
            wasm_hash: Some(
                "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
            ),
            ..Default::default()
        };
        let res_none = cache::query_cache(&filter_none).expect("query");
        assert!(res_none.is_empty());
    });
}

#[test]
fn test_query_cache_function_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000",
            "transfer",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000",
            "approve",
            &[],
            "testnet",
            200_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );

        let filter = cache::CacheFilter {
            function: Some("transfer".to_string()),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].function, "transfer");

        // Exact match semantics: substring should NOT match
        let filter_sub = cache::CacheFilter {
            function: Some("trans".to_string()),
            ..Default::default()
        };
        let res_sub = cache::query_cache(&filter_sub).expect("query");
        assert!(res_sub.is_empty());
    });
}

#[test]
fn test_query_cache_network_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000aaaa0000",
            "fn1",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000",
            "fn2",
            &[],
            "mainnet",
            200_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );

        let filter = cache::CacheFilter {
            network: Some("testnet".to_string()),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].network, "testnet");

        let filter_main = cache::CacheFilter {
            network: Some("mainnet".to_string()),
            ..Default::default()
        };
        let res_main = cache::query_cache(&filter_main).expect("query");
        assert_eq!(res_main.len(), 1);
        assert_eq!(res_main[0].network, "mainnet");
    });
}

#[test]
fn test_query_cache_min_fee_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111000011110000111100001111000011110000111100001111000011110000",
            "low_fee",
            &[],
            "testnet",
            50_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "2222000022220000222200002222000022220000222200002222000022220000",
            "mid_fee",
            &[],
            "testnet",
            100_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "3333000033330000333300003333000033330000333300003333000033330000",
            "high_fee",
            &[],
            "testnet",
            200_000,
            30_000,
            "2026-01-03T00:00:00Z",
        );

        let filter = cache::CacheFilter {
            min_fee: Some(100_000),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 2);
        assert!(res.iter().all(|e| e.total_stroops >= 100_000));
    });
}

#[test]
fn test_query_cache_max_fee_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111000011110000111100001111000011110000111100001111000011110000",
            "low_fee",
            &[],
            "testnet",
            50_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "2222000022220000222200002222000022220000222200002222000022220000",
            "mid_fee",
            &[],
            "testnet",
            100_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "3333000033330000333300003333000033330000333300003333000033330000",
            "high_fee",
            &[],
            "testnet",
            200_000,
            30_000,
            "2026-01-03T00:00:00Z",
        );

        let filter = cache::CacheFilter {
            max_fee: Some(100_000),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 2);
        assert!(res.iter().all(|e| e.total_stroops <= 100_000));
    });
}

#[test]
fn test_query_cache_fee_range_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111000011110000111100001111000011110000111100001111000011110000",
            "low_fee",
            &[],
            "testnet",
            50_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "2222000022220000222200002222000022220000222200002222000022220000",
            "mid_fee",
            &[],
            "testnet",
            100_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "3333000033330000333300003333000033330000333300003333000033330000",
            "high_fee",
            &[],
            "testnet",
            200_000,
            30_000,
            "2026-01-03T00:00:00Z",
        );

        let filter = cache::CacheFilter {
            min_fee: Some(75_000),
            max_fee: Some(150_000),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].function, "mid_fee");
        assert_eq!(res[0].total_stroops, 100_000);
    });
}

#[test]
fn test_query_cache_since_filter() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111000011110000111100001111000011110000111100001111000011110000",
            "early",
            &[],
            "testnet",
            100_000,
            10_000,
            "2025-12-31T23:59:59Z",
        );
        seed_query_test_entry(
            tmp,
            "2222000022220000222200002222000022220000222200002222000022220000",
            "on_boundary",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "3333000033330000333300003333000033330000333300003333000033330000",
            "late",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-02T12:00:00Z",
        );

        let since_dt = cache::parse_since_timestamp("2026-01-01").expect("parse since");
        let filter = cache::CacheFilter {
            since: Some(since_dt),
            ..Default::default()
        };
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(res.len(), 2);
        assert!(res.iter().all(|e| e.function != "early"));
    });
}

#[test]
fn test_query_cache_multiple_filters_and_semantics() {
    with_temp_home(|tmp| {
        // Entry 1: matches ALL criteria
        seed_query_test_entry(
            tmp,
            "deadbeef00000000deadbeef00000000deadbeef00000000deadbeef00000000",
            "transfer",
            &["arg1"],
            "testnet",
            150_000,
            10_000,
            "2026-02-01T12:00:00Z",
        );
        // Entry 2: wrong function
        seed_query_test_entry(
            tmp,
            "deadbeef00000000deadbeef00000000deadbeef00000000deadbeef00000000",
            "mint",
            &["arg2"],
            "testnet",
            150_000,
            10_000,
            "2026-02-01T12:00:00Z",
        );
        // Entry 3: wrong network
        seed_query_test_entry(
            tmp,
            "deadbeef00000000deadbeef00000000deadbeef00000000deadbeef00000000",
            "transfer",
            &["arg3"],
            "mainnet",
            150_000,
            10_000,
            "2026-02-01T12:00:00Z",
        );
        // Entry 4: fee too low
        seed_query_test_entry(
            tmp,
            "deadbeef00000000deadbeef00000000deadbeef00000000deadbeef00000000",
            "transfer",
            &["arg4"],
            "testnet",
            50_000,
            10_000,
            "2026-02-01T12:00:00Z",
        );
        // Entry 5: timestamp too early
        seed_query_test_entry(
            tmp,
            "deadbeef00000000deadbeef00000000deadbeef00000000deadbeef00000000",
            "transfer",
            &["arg5"],
            "testnet",
            150_000,
            10_000,
            "2025-12-01T12:00:00Z",
        );

        let since_dt = cache::parse_since_timestamp("2026-01-01").expect("parse since");
        let filter = cache::CacheFilter {
            wasm_hash: Some("deadbeef".to_string()),
            function: Some("transfer".to_string()),
            network: Some("testnet".to_string()),
            min_fee: Some(100_000),
            max_fee: Some(200_000),
            since: Some(since_dt),
            to: None,
        };

        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(
            res.len(),
            1,
            "only entry satisfying all AND filters should match"
        );
        assert_eq!(res[0].function, "transfer");
        assert_eq!(res[0].network, "testnet");
        assert_eq!(res[0].total_stroops, 150_000);
    });
}

#[test]
fn test_query_cache_no_filters_returns_all() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111000011110000111100001111000011110000111100001111000011110000",
            "fn1",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );
        seed_query_test_entry(
            tmp,
            "2222000022220000222200002222000022220000222200002222000022220000",
            "fn2",
            &[],
            "mainnet",
            200_000,
            20_000,
            "2026-01-02T00:00:00Z",
        );

        let filter = cache::CacheFilter::default();
        let res = cache::query_cache(&filter).expect("query");
        assert_eq!(
            res.len(),
            2,
            "no filters should return all cached estimates across all networks"
        );
    });
}

#[test]
fn test_query_cache_invalid_inputs_return_app_error() {
    // Invalid network
    let filter_net = cache::CacheFilter {
        network: Some("nonexistent_network".to_string()),
        ..Default::default()
    };
    assert!(cache::query_cache(&filter_net).is_err());

    // Negative min fee
    let filter_fee_neg = cache::CacheFilter {
        min_fee: Some(-10),
        ..Default::default()
    };
    assert!(cache::query_cache(&filter_fee_neg).is_err());

    // Invalid fee range (min > max)
    let filter_fee_range = cache::CacheFilter {
        min_fee: Some(200_000),
        max_fee: Some(100_000),
        ..Default::default()
    };
    assert!(cache::query_cache(&filter_fee_range).is_err());

    // Invalid wasm hash (non-hex)
    let filter_hash = cache::CacheFilter {
        wasm_hash: Some("not_hexadecimal!".to_string()),
        ..Default::default()
    };
    assert!(cache::query_cache(&filter_hash).is_err());

    // Empty function name
    let filter_fn = cache::CacheFilter {
        function: Some(String::new()),
        ..Default::default()
    };
    assert!(cache::query_cache(&filter_fn).is_err());

    // Invalid date parsing
    assert!(cache::parse_since_timestamp("invalid-date-string").is_err());
    assert!(cache::parse_to_timestamp("2026-99-99").is_err());
}

#[test]
fn test_query_cache_read_only() {
    with_temp_home(|tmp| {
        seed_query_test_entry(
            tmp,
            "1111000011110000111100001111000011110000111100001111000011110000",
            "fn1",
            &[],
            "testnet",
            100_000,
            10_000,
            "2026-01-01T00:00:00Z",
        );

        let filter = cache::CacheFilter::default();
        let _ = cache::query_cache(&filter).expect("first query");
        let _ = cache::query_cache(&filter).expect("second query");

        // Verify count and entry remains unaltered
        let stats = cache::cache_stats().expect("cache stats");
        assert_eq!(stats.total_entries, 1);
        let entry = cache::load_estimate(
            "1111000011110000111100001111000011110000111100001111000011110000",
            "fn1",
            &[],
        )
        .expect("load")
        .expect("exists");
        assert_eq!(entry.timestamp, "2026-01-01T00:00:00Z");
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Ledger I/O footprint (#278)
// ─────────────────────────────────────────────────────────────────────────

fn sample_io() -> cache::IoFootprint {
    cache::IoFootprint {
        read_entries: 3,
        write_entries: 2,
        read_bytes: 40,
        write_bytes: 136,
    }
}

#[test]
fn test_save_estimate_with_io_roundtrip() {
    with_temp_home(|_tmp| {
        let io = sample_io();
        cache::save_estimate_with_io("h", "f", &[], "testnet", 7, 500, 20, 10, io, Some(12), true)
            .expect("save with io");

        let loaded = cache::load_estimate("h", "f", &[])
            .expect("load")
            .expect("entry should exist");
        assert_eq!(loaded.io, Some(io));
        assert_eq!(loaded.duration_ms, Some(12));
    });
}

#[test]
fn test_save_estimate_without_io_leaves_footprint_absent() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h", "f", &[], "testnet", 1, 100, 10, 5, None, true).expect("save");

        let loaded = cache::load_estimate("h", "f", &[])
            .expect("load")
            .expect("entry should exist");
        assert!(
            loaded.io.is_none(),
            "save_estimate must not invent an I/O footprint"
        );
    });
}

/// Re-running the same key replaces the stored footprint along with the other
/// metrics.
#[test]
fn test_save_estimate_with_io_updates_existing_entry() {
    with_temp_home(|_tmp| {
        cache::save_estimate("h", "f", &[], "testnet", 1, 100, 10, 5, None, true).expect("first");

        let io = sample_io();
        cache::save_estimate_with_io("h", "f", &[], "testnet", 2, 200, 20, 10, io, None, true)
            .expect("second");

        let loaded = cache::load_estimate("h", "f", &[])
            .expect("load")
            .expect("entry should exist");
        assert_eq!(loaded.ledger, 2);
        assert_eq!(loaded.io, Some(io));
    });
}

/// `export_cached_estimates` reads every column the row mapping needs,
/// including the ones added after the initial schema.
#[test]
fn test_export_cached_estimates_includes_footprint() {
    with_temp_home(|_tmp| {
        let io = sample_io();
        cache::save_estimate_with_io("h", "f", &[], "testnet", 3, 42, 7, 9, io, Some(5), true)
            .expect("save");

        let exported = cache::export_cached_estimates().expect("export");
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].io, Some(io));
        assert_eq!(exported[0].duration_ms, Some(5));
        assert!(exported[0].success);
    });
}

/// A database written by a build that predates the `io_json` column is
/// upgraded in place when the library opens it, and its rows still load.
#[test]
fn test_legacy_database_without_io_column_still_loads() {
    with_temp_home(|home| {
        let dir = home.join(".soroban-cost-estimator");
        std::fs::create_dir_all(&dir).expect("create data dir");
        let db = dir.join("cache.db");

        // The schema exactly as it existed before `io_json` was added.
        {
            let conn = rusqlite::Connection::open(&db).expect("open cache db");
            conn.execute_batch(
                "CREATE TABLE estimates (
                    version          INTEGER NOT NULL,
                    wasm_hash        TEXT NOT NULL,
                    function         TEXT NOT NULL,
                    args_hash        TEXT NOT NULL,
                    network          TEXT NOT NULL,
                    ledger           INTEGER NOT NULL,
                    total_stroops    INTEGER NOT NULL,
                    cpu_instructions INTEGER NOT NULL,
                    memory_bytes     INTEGER NOT NULL,
                    timestamp        TEXT NOT NULL,
                    duration_ms      INTEGER,
                    success          INTEGER NOT NULL DEFAULT 1,
                    PRIMARY KEY (wasm_hash, function, args_hash)
                );",
            )
            .expect("create legacy schema");
            conn.execute(
                "INSERT INTO estimates \
                 (version, wasm_hash, function, args_hash, network, ledger, total_stroops, cpu_instructions, memory_bytes, timestamp, success) \
                 VALUES (?1, 'old', 'legacy_fn', ?2, 'testnet', 3, 100, 10, 5, '2026-01-01T00:00:00Z', 1)",
                rusqlite::params![
                    cache::CACHE_SCHEMA_VERSION as i64,
                    args_hash_of(&["a"]),
                ],
            )
            .expect("insert legacy row");
        }

        let loaded = cache::load_estimate("old", "legacy_fn", &["a".to_string()])
            .expect("load legacy entry")
            .expect("legacy entry should exist");
        assert_eq!(loaded.total_stroops, 100);
        assert!(
            loaded.io.is_none(),
            "a pre-io_json row has no footprint to report"
        );
    });
}
