# WASM Parser & Contract-Spec Decoding Fuzz Harness

This directory houses the `cargo-fuzz` target (`wasm_parser`) covering:

- `soroban_cost_estimator::wasm::parser::validate_wasm`
- `soroban_cost_estimator::wasm::parser::enumerate_module_metadata`
- `soroban_cost_estimator::wasm::parser::parse_contract_spec_entries`
- `soroban_cost_estimator::wasm::parser::parse_contract_meta`
- `soroban_cost_estimator::wasm::parser::section_size_breakdown`
- `soroban_cost_estimator::xdr_helper::decode_config_entry_xdr`

Inputs are capped at `MAX_FUZZ_WASM_INPUT_BYTES` (64 KiB) and XDR decoding recursion is bounded by `MAX_XDR_DECODE_DEPTH` (64).

## Running a Short Local Fuzz Session

### 1. Property & Seed-Corpus Harness (Stable Rust, No Nightly Required)

```bash
cargo test --test parser_tests fuzz_
```

### 2. LibFuzzer Session via `cargo-fuzz` (Nightly Rust)

Seed the corpus from the committed WASM fixtures and run a 30-second bounded fuzz session:

```bash
mkdir -p fuzz/corpus/wasm_parser
cp tests/fixtures/minimal.wasm tests/fixtures/zeroarg.wasm tests/fixtures/contract.wasm fuzz/corpus/wasm_parser/
cargo +nightly fuzz run wasm_parser -- -max_total_time=30 -max_len=65536
```

## Adding Regression Inputs

When a crashing or panicking input is discovered:

1. Minimize the artifact (optional):
   ```bash
   cargo +nightly fuzz tmin wasm_parser fuzz/artifacts/wasm_parser/crash-<hash>
   ```
2. Add a deterministic regression test in `tests/parser_tests.rs` (under the `fuzz_` regression section) passing the bytes to `fuzz_wasm_parser_and_spec_boundaries` and asserting that the specific parser function returns an `Err(...)` instead of panicking.
3. Run `cargo test --test parser_tests` to verify the fix before committing.
