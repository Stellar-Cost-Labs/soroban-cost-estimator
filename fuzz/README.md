# WASM Parser & Contract-Spec Fuzzing

This directory contains `cargo-fuzz` targets for fuzzing the WebAssembly parser and Soroban contract-spec decoding boundaries.

## Target Coverage

The fuzz target `fuzz_wasm_parser` tests:
- `validate_wasm`: Validates WebAssembly binary structure and section parsing.
- `validate_wasm_memory_limits`: Verifies memory constraints against Soroban limits.
- `enumerate_module_metadata`: Inspects section layout, function imports/exports, and structural properties.
- `parse_contract_spec_entries`: Decodes custom `contractspecv0` sections containing XDR-encoded contract specs.

Inputs are bounded to 64 KiB (`MAX_FUZZ_INPUT_SIZE`) to ensure iterations remain efficient and focus on parser and decoder edge cases rather than unbounded allocations. All fuzz operations execute purely in-memory with no network access.

## Prerequisites

Install `cargo-fuzz` and a Rust nightly toolchain:

```bash
cargo install cargo-fuzz
rustup toolchain install nightly
```

## Running a Local Fuzz Session

To run a short local fuzzing session (e.g. 30 seconds):

```bash
cargo +nightly fuzz run fuzz_wasm_parser -- -max_total_time=30 -max_len=65536
```

To run with AddressSanitizer and LeakSanitizer (default on Linux/macOS):

```bash
cargo +nightly fuzz run fuzz_wasm_parser
```

## Seed Corpus

The seed corpus is located in `fuzz/corpus/fuzz_wasm_parser/` and is pre-seeded with valid Soroban contracts and minimal binaries:
- `minimal.wasm`: Smallest valid WebAssembly module.
- `zeroarg.wasm`: Valid module with a zero-argument export.
- `contract.wasm`: Real compiled Soroban smart contract with typed `contractspecv0` section.

## Adding Regression Inputs

When `cargo-fuzz` discovers a crash, panic, or unhandled error, it saves the reproducing input to:
`fuzz/artifacts/fuzz_wasm_parser/crash-<hash>`

To add a regression test:
1. Copy the crash artifact into `fuzz/corpus/fuzz_wasm_parser/` or a dedicated test fixture.
2. Add a unit test in `tests/parser_tests.rs` feeding the input directly to `validate_wasm`, `enumerate_module_metadata`, and `parse_contract_spec_entries`.
3. Assert that the parser returns an expected error variant (`AppError::WasmValidation` or `AppError::WasmParse`) without panicking or hanging.
4. Run `cargo test --workspace` to verify that standard test runs catch regressions deterministically.
