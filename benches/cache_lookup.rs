//! Cache storage benchmarks: SQLite backend vs the pre-SQLite JSON store.
//!
//! Issue #334 replaces a directory of one JSON file per estimate with a
//! single indexed SQLite database. These benchmarks demonstrate the
//! difference directly: the same 500 entries are written to both backends,
//! then point lookups and per-network listings are measured on each.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench cache_lookup
//! ```

use std::path::PathBuf;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use soroban_cost_estimator::cache;

/// Number of entries seeded into each backend.
const ENTRIES: usize = 500;

/// Directory holding the JSON baseline used for comparison. Kept next to the
/// temporary HOME so the benchmark never touches the developer's data.
fn json_baseline_dir(home: &std::path::Path) -> PathBuf {
    home.join("json-baseline")
}

/// Set `HOME`/`USERPROFILE` so the library's data directory resolves inside
/// `home`. Called once during benchmark setup, before any measured code runs.
fn point_data_dir_at(home: &std::path::Path) {
    // SAFETY: benchmarks run single-threaded during setup, and nothing else
    // in this process reads these environment variables.
    unsafe {
        std::env::set_var("HOME", home);
        std::env::set_var("USERPROFILE", home);
    }
}

/// A representative estimate payload.
fn payload(i: usize) -> (&'static str, String, String, i64) {
    (
        "testnet",
        format!("hash-{i:04}"),
        format!("func-{i:04}"),
        1_000 + i as i64,
    )
}

/// Seed the SQLite cache with `ENTRIES` rows.
fn seed_sqlite() {
    for i in 0..ENTRIES {
        let (network, wasm_hash, function, stroops) = payload(i);
        cache::save_estimate(
            &wasm_hash,
            &function,
            &[],
            network,
            i as u32,
            stroops,
            10_000,
            1_000,
            None,
            true,
        )
        .expect("seed sqlite cache");
    }
}

/// Seed a JSON file per entry, mirroring the pre-SQLite store's layout.
fn seed_json(home: &std::path::Path) {
    let dir = json_baseline_dir(home);
    std::fs::create_dir_all(&dir).expect("create json baseline dir");
    for i in 0..ENTRIES {
        let (network, wasm_hash, function, stroops) = payload(i);
        let body = format!(
            r#"{{"wasm_hash":"{wasm_hash}","function":"{function}","args_hash":"","network":"{network}","ledger":{i},"total_stroops":{stroops},"cpu_instructions":10000,"memory_bytes":1000,"timestamp":"2026-01-01T00:00:00Z"}}"#
        );
        std::fs::write(dir.join(format!("{wasm_hash}-{function}.json")), body)
            .expect("write json entry");
    }
}

/// Find one entry by scanning JSON files — the O(n) Filesystem walk the
/// SQLite index replaces.
fn json_scan_lookup(home: &std::path::Path, wasm_hash: &str, function: &str) -> usize {
    let dir = json_baseline_dir(home);
    let wanted = format!("{wasm_hash}-{function}.json");
    let entries = std::fs::read_dir(&dir).expect("read json dir");
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy() == wanted {
            let raw = std::fs::read_to_string(entry.path()).expect("read json entry");
            let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse json entry");
            return parsed["total_stroops"].as_i64().unwrap_or(0) as usize;
        }
    }
    0
}

/// Read and parse every JSON file — the whole-directory scan the indexed
/// `network` listing replaces.
fn json_scan_listing(home: &std::path::Path) -> usize {
    let dir = json_baseline_dir(home);
    let entries = std::fs::read_dir(&dir).expect("read json dir");
    let mut count = 0usize;
    for entry in entries.flatten() {
        let raw = std::fs::read_to_string(entry.path()).expect("read json entry");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse json entry");
        if parsed["network"] == "testnet" {
            count += 1;
        }
    }
    count
}

fn bench_cache(c: &mut Criterion) {
    let home = std::env::temp_dir().join(format!("sce-cache-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create bench home");
    point_data_dir_at(&home);
    seed_sqlite();
    seed_json(&home);

    let mut group = c.benchmark_group("cache_lookup");
    group.bench_function("sqlite_indexed_point_lookup", |b| {
        b.iter(|| {
            let loaded = cache::load_estimate(
                black_box("hash-0250"),
                black_box("func-0250"),
                black_box(&[]),
            )
            .expect("sqlite lookup")
            .expect("seeded row");
            black_box(loaded.total_stroops)
        });
    });
    group.bench_function("json_file_lookup", |b| {
        b.iter(|| {
            black_box(json_scan_lookup(
                black_box(&home),
                black_box("hash-0250"),
                black_box("func-0250"),
            ))
        });
    });
    group.finish();

    let mut group = c.benchmark_group("cache_listing");
    group.bench_function("sqlite_network_listing", |b| {
        b.iter(|| {
            let rows = cache::list_cached_estimates(black_box("testnet")).expect("sqlite listing");
            black_box(rows.len())
        });
    });
    group.bench_function("json_directory_listing", |b| {
        b.iter(|| black_box(json_scan_listing(black_box(&home))));
    });
    group.finish();

    // LRU eviction: hold the cache at a 100-entry quota and keep writing new
    // keys, so every save past the quota performs a real eviction pass.
    cache::set_cache_limits(cache::CacheLimits {
        max_bytes: 0,
        max_entries: 100,
    })
    .expect("set bench cache limits");

    let mut group = c.benchmark_group("lru_eviction");
    let next_key = std::sync::atomic::AtomicUsize::new(10_000);
    group.bench_function("save_beyond_quota", |b| {
        b.iter(|| {
            let i = next_key.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            cache::save_estimate(
                &format!("evict-hash-{i}"),
                "evict-func",
                &[],
                "testnet",
                1,
                1_000,
                10,
                5,
                None,
                true,
            )
            .expect("save beyond quota");
        });
    });
    group.finish();

    let _ = std::fs::remove_dir_all(&home);
}

criterion_group!(benches, bench_cache);
criterion_main!(benches);
