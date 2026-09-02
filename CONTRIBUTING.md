# Contributing to Soroban Cost Estimator

We welcome contributions! Here's how to get started.

## Development Setup
<!-- contibuting -->
```bash
# Clone the repo
git clone https://github.com/aigbagbobila/soroban-cost-estimator.git
cd soroban-cost-estimator

# Build
cargo build

# Generate test WASM fixture
cargo run --bin gen_test_wasm

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets
```

## Coding Standards

1. **No `unwrap()` or `expect()` outside `tests/`** — every RPC call, XDR
   decode, and file I/O must return a `Result` through the `AppError` enum.
2. **No floats near fee math** — stroops are integers; use basis-points-style
   integer math for any percentage margin.
3. **No hardcoded stroops-per-resource-unit constants** — every fee-relevant
   constant must come from a fetched `ConfigSetting*` entry or from
   `simulateTransaction`'s own output.
4. **Doc comments** on every public function stating what it fetches/computes
   and what network calls it makes, if any.
5. **`clippy::all` and `clippy::pedantic` clean** — enforced in CI.

## Pull Request Process

1. One commit per logical unit.
2. Conventional commit format: `type(scope): description`
   (e.g. `feat(rpc): add getLedgerEntries config fetch`).
3. Rebase onto the latest `main` before pushing.
4. Ensure CI passes (build, clippy, tests).
5. Request review from a maintainer.

## Project Structure

```
src/
  main.rs            # Binary entry point
  lib.rs             # Library crate (re-exports all modules)
  cli.rs             # Clap command definitions
  wasm/              # WASM loading, validation, function enumeration
  rpc/               # RPC client, simulateTransaction, getLedgerEntries
  config_snapshot/   # Config model, snapshot store, diff logic
  report/            # Cost report formatting, fee calculation
  xdr_helper.rs      # Stellar XDR encode/decode helpers
  error.rs           # Single error enum
tests/
  fixtures/          # Test WASM files
  parser_tests.rs
  config_diff_tests.rs
```

## License

By contributing, you agree that your contributions will be licensed under
the [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) license.
