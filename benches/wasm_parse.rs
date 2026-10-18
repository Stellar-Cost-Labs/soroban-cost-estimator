//! Parser and contract-spec decoding benchmarks.
//!
//! Benchmark inputs live in `tests/fixtures/` (`minimal.wasm` and `contract.wasm`).
//! To regenerate synthetic fixtures deterministically:
//!   `cargo run --bin gen_test_wasm`
//! To verify committed fixtures without overwriting them:
//!   `cargo run --bin gen_test_wasm -- --check`
//!   `tests/fixtures/contract/build.sh --check`

use std::path::Path;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use soroban_cost_estimator::wasm::parser::{enumerate_functions, load_wasm, parse_contract_spec};

const EXPECTED_MINIMAL_WASM_SHA256: &str =
    "8f581f8899890a7479c20494f53cc5d60156a4d42e39d6a587a9f3089f198fe0";
const EXPECTED_CONTRACT_WASM_SHA256: &str =
    "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd";

fn bench_wasm_parsing(c: &mut Criterion) {
    let contract_path = Path::new("tests/fixtures/contract.wasm");
    let minimal_path = Path::new("tests/fixtures/minimal.wasm");

    let contract_info = load_wasm(contract_path).expect("failed to load contract.wasm");
    let minimal_info = load_wasm(minimal_path).expect("failed to load minimal.wasm");
    assert_eq!(
        contract_info.wasm_hash, EXPECTED_CONTRACT_WASM_SHA256,
        "stale contract.wasm fixture; run `cargo run --bin gen_test_wasm -- --check`"
    );
    assert_eq!(
        minimal_info.wasm_hash, EXPECTED_MINIMAL_WASM_SHA256,
        "stale minimal.wasm fixture; run `cargo run --bin gen_test_wasm -- --check`"
    );

    // Load the bytes beforehand for the pure in-memory parsing benchmarks
    let contract_bytes = contract_info.bytes;
    let minimal_bytes = minimal_info.bytes;

    // --- WASM Loading Benchmarks (includes file I/O and full parsing pipeline) ---
    let mut group = c.benchmark_group("wasm_loading");
    group.bench_function("contract_wasm", |b| {
        b.iter(|| load_wasm(black_box(contract_path)));
    });
    group.bench_function("minimal_wasm", |b| {
        b.iter(|| load_wasm(black_box(minimal_path)));
    });
    group.finish();

    // --- Function Enumeration Benchmarks (pure in-memory) ---
    let mut group = c.benchmark_group("function_enumeration");
    group.bench_function("contract_wasm", |b| {
        b.iter(|| enumerate_functions(black_box(&contract_bytes)));
    });
    group.bench_function("minimal_wasm", |b| {
        b.iter(|| enumerate_functions(black_box(&minimal_bytes)));
    });
    group.finish();

    // --- Spec Parsing Benchmarks (pure in-memory) ---
    let mut group = c.benchmark_group("spec_parsing");
    group.bench_function("contract_wasm", |b| {
        b.iter(|| parse_contract_spec(black_box(&contract_bytes)));
    });
    group.bench_function("minimal_wasm", |b| {
        b.iter(|| parse_contract_spec(black_box(&minimal_bytes)));
    });
    group.finish();
}

criterion_group!(benches, bench_wasm_parsing);
criterion_main!(benches);
