# Contributing

Contributions are welcome, and the project is set up for scoped, low-friction
first contributions.

## Start with the issue backlog

The fastest path to useful work is the issue tracker — every open issue
carries a Summary, Acceptance Criteria, and Implementation Hints, sized for a
focused sprint:

- Issue backlog: <https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues>
- List open issues from the CLI:
  ```bash
  gh issue list -R Stellar-Cost-Labs/soroban-cost-estimator --state open
  ```

If you are arriving from a Drips Wave contributor sprint, issues labeled
`Stellar Wave` include detailed acceptance criteria ready for pickup.

## Prerequisites

- **Rust**: `1.85+` (Edition 2024) via [`rustup`](https://rustup.rs/)
- **Components**: `rustfmt` and `clippy` (`rustup component add rustfmt clippy`)
- **Git**: `2.30+`

## Development setup and standards

The full contributing guide — coding standards, the PR process, and the
project structure — lives in [`CONTRIBUTING.md`](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/blob/main/CONTRIBUTING.md).
In short:

- One commit per logical unit, conventional commit format
  (`feat(rpc): …`, `fix(fee-calc): …`).
- No `unwrap()` or `expect()` outside `#[cfg(test)]` / `tests/`; all fallible
  operations return `Result` through the `AppError` enum.
- Integer-only stroop math (`i64` / `i128`) for fee calculations, and no
  hardcoded stroops-per-unit constants — every fee-relevant constant must come
  from a fetched `ConfigSetting*` entry or from `simulateTransaction`'s own
  output.
- `rustfmt` and `clippy` clean, enforced in CI.

```bash
# Format check
cargo fmt --all -- --check

# Lint all targets
cargo clippy --workspace --all-targets -- -D warnings

# Run unit and integration tests
cargo test --workspace

# Run Criterion benchmarks
cargo bench --bench wasm_benchmark
```

## PR Submission Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] No `unwrap()` / `expect()` in `src/` outside test modules
- [ ] PR description references the issue (`Closes #<number>`)

`main` is protected: the CI checks must be green for changes to merge.
