# Contributing to Soroban Cost Estimator

We welcome contributions from community members and Drips Wave / Stellar Wave contributors! This guide walks through local environment setup, running tests and benchmarks, coding standards, and the pull request submission checklist.

## Prerequisites

Ensure the following tools are installed before starting development:

- **Rust Toolchain**: Stable Rust `1.85+` (Edition 2024) installed via [`rustup`](https://rustup.rs/)
- **Code Formatter (`rustfmt`)**: `rustup component add rustfmt`
- **Linter (`clippy`)**: `rustup component add clippy`
- **Git**: `2.30+` for branching, rebasing, and submitting pull requests

Verify your toolchain:

```bash
rustc --version
cargo fmt --version
cargo clippy --version
git --version
```

## Development Setup
<!-- contibuting -->
```bash
# 1. Fork and clone the repository
git clone https://github.com/Stellar-Cost-Labs/soroban-cost-estimator.git
cd soroban-cost-estimator

# 2. Build the workspace
cargo build

# 3. (Optional) Regenerate test WASM fixtures
cargo run --bin gen_test_wasm
```

## Running Tests, Linters, and Benchmarks

Run the verification suite locally before opening a pull request:

```bash
# Format check (enforced in CI)
cargo fmt --all -- --check

# Clippy lints across all workspace targets (enforced in CI)
cargo clippy --workspace --all-targets -- -D warnings

# Run all unit and integration tests
cargo test --workspace

# Run only library unit tests
cargo test --lib

# Run a specific integration test suite (e.g., CLI, parser, or config diff tests)
cargo test --test cli_tests
cargo test --test parser_tests
cargo test --test config_diff_tests

# Run Criterion benchmarks (WASM parsing and SHA-256 hashing)
cargo bench --bench wasm_benchmark
```

## Coding Standards

1. **No `unwrap()` or `expect()` outside `#[cfg(test)]` / `tests/`** — every RPC call, XDR decode, and file I/O operation in `src/` must return a `Result` through the [`AppError`](src/error.rs) enum with clear context.
2. **Integer-only arithmetic for fee calculations (`i64` / `i128` stroops)** — never use floating-point (`f64`) math when computing resource fees or stroop totals (`1 XLM = 10,000,000 stroops`). Use integer scaling and basis-points/permilles arithmetic for percentages and margins.
3. **No hardcoded stroops-per-resource-unit constants** — every fee-relevant constant must come from a fetched `ConfigSetting*` entry or from `simulateTransaction`'s own output.
4. **Documentation comments** — add `///` rustdoc comments on every public function, struct, and enum explaining what it computes or fetches and what network calls it makes, if any.
5. **Conventional Commits** — format commit messages as `type(scope): description`:
   - `feat(cli): add fee and resource budget gates for CI`
   - `fix(rpc): handle transient HTTP 503 gateway errors`
   - `refactor(config): extract shared live config diff pipeline`
   - `docs(contributing): add PR review checklist`
   - `test(fee): add regression test for refundable fee floor`
6. **`clippy::all` and `rustfmt` clean** — all code must compile with zero warnings under `cargo clippy --workspace --all-targets -- -D warnings`.

## Pull Request Process & Checklist

1. Create a descriptive feature/fix branch from `main` (e.g., `git checkout -b feat/my-feature`).
2. Keep commits focused (one logical change per commit) using Conventional Commit messages.
3. Rebase onto the latest `main` before pushing.
4. Reference the issue in your PR description (e.g., `Closes #123`).

### PR Submission Checklist

Before requesting a maintainer review, confirm all items below:

- [ ] `cargo fmt --all -- --check` passes with no formatting diffs
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` reports zero warnings
- [ ] `cargo test --workspace` passes 100% (unit + integration tests)
- [ ] No `unwrap()` or `expect()` introduced in non-test code under `src/`
- [ ] Fee math uses integer-only stroop arithmetic (`i64` / `i128`)
- [ ] New flags, subcommands, or public APIs include unit/integration tests and documentation
- [ ] PR description includes `Closes #<issue-number>`

## Project Structure

```text
src/
  main.rs            # Binary entry point & command dispatch
  lib.rs             # Library crate (re-exports all modules)
  cli.rs             # Clap command & argument definitions
  cache.rs           # On-disk SQLite/JSON estimate cache & TTL management
  interactive.rs     # Interactive terminal prompt mode (-i)
  paths.rs           # Cross-platform config & cache directory resolution
  wasm/              # WASM loading, validation, spec & metadata parsing
  rpc/               # JSON-RPC & WebSocket client, retry helper, simulateTransaction, getLedgerEntries
  config_snapshot/   # Config model, snapshot store, history, and diff engine
  report/            # Cost report formatting (table/json/csv/markdown), fee calculation, diffing
  xdr_helper.rs      # Stellar XDR encode/decode helpers
  error.rs           # Unified AppError enum
tests/
  fixtures/          # Test WASM binaries
  cli_tests.rs       # End-to-end CLI integration tests
  parser_tests.rs    # WASM parser & spec extraction tests
  config_diff_tests.rs
benches/
  wasm_benchmark.rs  # Criterion benchmarks for WASM loading & hashing
```

## License

By contributing, you agree that your contributions will be licensed under
the [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) license.
