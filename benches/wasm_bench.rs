//! Criterion micro-benchmarks for the WASM inspection pipeline.
//!
//! Covers the three stages that future work on WASM inspection and spec type
//! validation is most likely to touch, so a regression in any of them shows up
//! as a Criterion diff rather than as a slow `estimate` run:
//!
//! * [`wasm_hashing`] — SHA-256 over the whole binary (the hash that keys the
//!   estimate cache), reported as throughput in MB/s.
//! * [`contractspecv0_decode`] — decoding the `contractspecv0` custom section
//!   into typed `ScSpecEntry` function signatures, reported as latency. The
//!   work is per-spec-entry rather than per-byte, so no `Throughput` is set and
//!   Criterion reports the time axis directly; at the fixture's size that axis
//!   lands in the microsecond range. (Criterion 0.5 picks the time unit
//!   automatically from the measured magnitude and exposes no knob to pin it to
//!   a specific unit.)
//! * [`wasm_section_traversal`] — the full `wasmparser` section walk that
//!   collects types, functions, memories, imports and exports, reported as
//!   throughput in MB/s.
//!
//! All three read the same fixture, `FIXTURE_PATH`: the repo's committed,
//! release-built `increment` contract that was actually deployed to testnet
//! (see `tests/fixtures/contract/README.md`). It is used in preference to
//! `tests/fixtures/minimal.wasm` because the point of these benchmarks is
//! realistic parse cost, and a 44-byte module produced by `gen_test_wasm` does
//! essentially no section traversal and carries no spec at all.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench wasm_benchmark
//! ```
//!
//! Note: this file is a `[[bench]]` target, not a `#[cfg(test)]` module, so it
//! follows the crate rule of no `unwrap()`/`expect()` outside test code. The
//! fixture is read once per benchmark function and a missing or unreadable
//! fixture reports and skips that benchmark instead of panicking.

use std::path::Path;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use soroban_cost_estimator::wasm::parser::{
    enumerate_module_metadata, parse_contract_spec, wasm_sha256_hex,
};

/// Path to the committed, deployable Soroban contract fixture.
const FIXTURE_PATH: &str = "tests/fixtures/contract.wasm";

/// Reads the WASM fixture used by every benchmark in this file.
///
/// The read happens outside every timed region — the benchmarks are about
/// in-memory parsing, not file I/O, and `load_wasm` is already covered by the
/// `wasm_parse` bench target. Returns `None` (after reporting the reason) when
/// the fixture is missing or unreadable, so a benchmark run degrades into
/// skipped benchmarks rather than a panic.
fn fixture_bytes() -> Option<Vec<u8>> {
    match std::fs::read(Path::new(FIXTURE_PATH)) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            eprintln!("wasm_benchmark: cannot read {FIXTURE_PATH}: {err}; skipping benchmarks");
            None
        }
    }
}

/// SHA-256 hashing of the whole WASM binary, as throughput in MB/s.
fn bench_wasm_hashing(c: &mut Criterion) {
    let Some(bytes) = fixture_bytes() else {
        return;
    };

    let mut group = c.benchmark_group("wasm_hashing");
    // Criterion derives MB/s from the per-iteration byte count.
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function("sha256_hex", |b| {
        b.iter(|| wasm_sha256_hex(black_box(&bytes)));
    });
    group.finish();
}

/// Decoding of the `contractspecv0` custom section, as latency.
fn bench_contractspecv0_decode(c: &mut Criterion) {
    let Some(bytes) = fixture_bytes() else {
        return;
    };

    // No `Throughput` here: this stage's cost scales with the number of spec
    // entries, not with the byte size of the module, so an MB/s figure would
    // be misleading. The reported time axis is the latency measurement.
    let mut group = c.benchmark_group("contractspecv0_decode");
    group.bench_function("parse_contract_spec", |b| {
        b.iter(|| parse_contract_spec(black_box(&bytes)));
    });
    group.finish();
}

/// The full WASM section walk, as throughput in MB/s.
fn bench_wasm_section_traversal(c: &mut Criterion) {
    let Some(bytes) = fixture_bytes() else {
        return;
    };

    let mut group = c.benchmark_group("wasm_section_traversal");
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function("enumerate_module_metadata", |b| {
        b.iter(|| enumerate_module_metadata(black_box(&bytes)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_wasm_hashing,
    bench_contractspecv0_decode,
    bench_wasm_section_traversal
);
criterion_main!(benches);
