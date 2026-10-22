use std::path::Path;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use soroban_cost_estimator::wasm::parser::{enumerate_functions, load_wasm, parse_contract_spec};

fn bench_wasm_parsing(c: &mut Criterion) {
    let contract_path = Path::new("tests/fixtures/contract.wasm");
    let minimal_path = Path::new("tests/fixtures/minimal.wasm");

    // Load the bytes beforehand for the pure in-memory parsing benchmarks
    let contract_bytes = std::fs::read(contract_path).expect("failed to read contract.wasm");
    let minimal_bytes = std::fs::read(minimal_path).expect("failed to read minimal.wasm");

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
