use std::path::PathBuf;
use std::sync::Mutex;

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

        // Corrupt entry 1: not JSON at all.
        // Corrupt entry 2: valid JSON but missing required fields.
        let dir = tmp.join(".soroban-cost-estimator").join("cache");
        std::fs::write(dir.join("garbage.json"), "{not json").expect("write garbage");
        std::fs::write(dir.join("wrong_shape.json"), r#"{"foo": 1}"#).expect("write wrong shape");

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 3, "should report every .json entry");

        let corrupt: Vec<&cache::CacheEntryStatus> = statuses.iter().filter(|s| !s.valid).collect();
        assert_eq!(corrupt.len(), 2, "both corrupted entries should be flagged");
        let names: Vec<&str> = corrupt.iter().map(|s| s.filename.as_str()).collect();
        assert!(names.contains(&"garbage.json"));
        assert!(names.contains(&"wrong_shape.json"));
    });
}

#[test]
fn test_verify_cache_ignores_non_json_files() {
    with_temp_home(|tmp| {
        cache::save_estimate("h1", "f1", &[], "testnet", 1, 100, 10, 5).expect("save f1");

        let dir = tmp.join(".soroban-cost-estimator").join("cache");
        std::fs::write(dir.join("notes.txt"), "not a cache entry").expect("write txt");

        let statuses = cache::verify_cache().expect("verify");
        assert_eq!(statuses.len(), 1, "only .json files should be checked");
        assert!(statuses[0].valid);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Concurrency
// ─────────────────────────────────────────────────────────────────────────

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
