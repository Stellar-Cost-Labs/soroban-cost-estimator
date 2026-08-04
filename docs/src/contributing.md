# Contributing

Contributions are welcome, and the project is set up for scoped, low-friction
first contributions.

## Start with the issue backlog

The fastest path to useful work is the issue tracker — every open issue
carries a Summary, Acceptance Criteria, and Tech Stack, sized for a focused
sprint:

- Issue backlog: <https://github.com/aigbagbobila/soroban-cost-estimator/issues>
- List open issues from the CLI:
  ```bash
  gh issue list -R aigbagbobila/soroban-cost-estimator --state open
  ```

Recent scoped work has included exponential RPC backoff, cache pruning and a
cache stats command, spec-typed `--arg` validation, parallelized
`estimate-all`, footprint reporting, and graceful watch shutdown. If you are
arriving from a Drips Wave contributor sprint, issues labeled `Stellar Wave`
are the ones with acceptance criteria ready for pickup.

## Development setup and standards

The full contributing guide — coding standards, the PR process, and the
project structure — lives in [`CONTRIBUTING.md`](https://github.com/aigbagbobila/soroban-cost-estimator/blob/main/CONTRIBUTING.md).
In short:

- One commit per logical unit, conventional commit format
  (`feat(rpc): …`, `fix(fee-calc): …`).
- No `unwrap()` or `expect()` outside `tests/`; all fallible operations
  return `Result` through the `AppError` enum.
- No floats near fee math, and no hardcoded stroops-per-unit constants —
  every fee-relevant constant must come from a fetched `ConfigSetting*`
  entry or from `simulateTransaction`'s own output.
- `clippy::all` + `clippy::pedantic` clean, enforced in CI.

```bash
cargo build
cargo test
cargo clippy --all-targets
```

`main` is protected: the CI `build` check must be green for changes to merge.
