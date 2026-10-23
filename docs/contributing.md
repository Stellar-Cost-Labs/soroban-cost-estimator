# Contributing

Contributions are welcome, and the project is set up for scoped, low-friction
first contributions. This page is the step-by-step version of
[`CONTRIBUTING.md`](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/blob/main/CONTRIBUTING.md);
follow it from a fresh clone to a green pull request.

## Contents

- [Pick an issue first](#pick-an-issue-first)
- [Prerequisites](#prerequisites)
- [Set up your fork](#set-up-your-fork)
- [Build and generate fixtures](#build-and-generate-fixtures)
- [Run the tests](#run-the-tests)
- [Lint and format](#lint-and-format)
- [Benchmarks](#benchmarks)
- [Coding standards](#coding-standards)
- [Commit conventions](#commit-conventions)
- [Open the pull request](#open-the-pull-request)
- [PR checklist](#pr-checklist)
- [Getting help](#getting-help)

## Pick an issue first

The fastest path to useful work is the issue tracker — every open issue carries
a Summary, Acceptance Criteria, and Tech Stack, sized for a focused sprint:

- Issue backlog: <https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues>
- List open issues from the CLI:
  ```bash
  gh issue list -R Stellar-Cost-Labs/soroban-cost-estimator --state open
  ```

Comment on the issue and wait for a maintainer to assign it before you start.
If you are arriving from a Drips Wave contributor sprint, issues labeled
`Stellar Wave` are the ones with acceptance criteria ready for pickup.

Recent scoped work has included exponential RPC backoff, cache pruning and a
cache stats command, spec-typed `--arg` validation, parallelized
`estimate-all`, footprint reporting, WebSocket config watching, and graceful
watch shutdown.

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Rust | **1.85+** | Edition 2024; the crate sets `rust-version = "1.85"` |
| `rustfmt` | Matches your toolchain | Required for the formatting gate |
| `clippy` | Matches your toolchain | Required for the lint gate |
| git | 2.30+ | Fork, rebase, and push workflow |
| `gh` CLI | Optional | Convenient for issues, PRs, and CI status |
| `wasm32v1-none` target | Optional | Only to **compile** Soroban contracts; not needed for the estimator or its tests |

Install a matching toolchain with rustup:

```bash
rustup toolchain install stable --component rustfmt clippy
rustup default stable
rustc --version   # 1.85.0 or newer
```

No database, Docker daemon, or local Stellar node is required. The test suite
uses mock RPC servers; live network access is only needed when you run the CLI
against testnet or mainnet yourself.

## Set up your fork

1. Fork <https://github.com/Stellar-Cost-Labs/soroban-cost-estimator> on GitHub.
2. Clone your fork and add the canonical repository as `upstream`:

   ```bash
   git clone https://github.com/<your-user>/soroban-cost-estimator.git
   cd soroban-cost-estimator
   git remote add upstream https://github.com/Stellar-Cost-Labs/soroban-cost-estimator.git
   git fetch upstream
   ```

3. Branch off the latest `main` with a descriptive name:

   ```bash
   git checkout main
   git pull --ff-only upstream main
   git checkout -b fix/cache-prune-quota
   ```

Use a `type/short-slug` name that matches the change: `feat/…`, `fix/…`,
`docs/…`, `test/…`, `refactor/…`, or `chore/…`.

## Build and generate fixtures

```bash
# Compile the binary, library, tests, and benchmarks
cargo build --all-targets

# Generate tests/fixtures/minimal.wasm and zeroarg.wasm
cargo run --bin gen_test_wasm
```

CI generates the fixtures before every test run. They are committed, but
regenerating keeps your working tree identical to CI's. Then make your change,
adding or updating the test that proves it.

## Run the tests

```bash
cargo run --bin gen_test_wasm
cargo test --workspace
```

While iterating, run a narrower set:

```bash
cargo test --lib                                  # module-level unit tests
cargo test --test parser_tests                    # one integration file
cargo test --test fee_calc_tests refundable       # one test by name
cargo test --doc                                  # doctests
```

The integration suites under `tests/` cover the cache, CLI, config diff and
history, error message style, fee calculation (including property tests), RPC
failover and mocks, the WASM parser, report snapshots, and user config.

The `insta` snapshot suites write `.snap.new` files when output changes.
Review them with `cargo insta review` (`cargo install cargo-insta`), accept only
the intended changes, and commit the updated `tests/snapshots/*.snap` files.

## Lint and format

These are the gates CI enforces:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --all-targets
```

`clippy::all` and `clippy::pedantic` are denied in `Cargo.toml`, so clippy must
finish warning-free. `cargo fmt --all` applies formatting in place.

If you have the optional subcommands installed, these mirror CI's extra jobs:

```bash
cargo audit        # dependency advisories
cargo deny check   # licenses, sources, bans, advisories
```

### If CI cannot reach crates.io

A job that fails before compiling with repeated
`Couldn't resolve host name (Could not resolve host: index.crates.io)` errors
hit a GitHub runner DNS failure, not a problem with your change. Re-run the
failed jobs from the PR's **Checks** tab (or `gh run rerun <run-id> --failed`).

## Benchmarks

Criterion benchmarks are not pass/fail gates, but they must compile:

```bash
cargo bench --no-run                # compile every benchmark only
cargo bench                         # run all of them
cargo bench --bench wasm_parse      # one target
cargo bench --bench cache_lookup
cargo bench --bench wasm_benchmark
```

Add `-- --quick` to a benchmark invocation while iterating. HTML reports are
written to `target/criterion/`.

## Coding standards

The full list, with examples, lives in
[`CONTRIBUTING.md`](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/blob/main/CONTRIBUTING.md#code-style-and-standards).
In short:

- **No `unwrap()` or `expect()` outside `#[cfg(test)]` modules and `tests/`.**
  Every RPC call, XDR decode, and file read returns `Result` through the
  `AppError` enum, with context about what failed.
- **Error messages follow one house style** — see the `# Message style` docs on
  `AppError` and `tests/error_tests.rs`. Lower-case the first word unless it is
  an acronym, lead with what failed, and never end with a period.
- **No floats near fee math.** Stroops are integers; use `i64`/`u64` and
  basis-points-style integer math for margins, with checked arithmetic.
- **No hardcoded stroops-per-resource-unit constants.** Every fee-relevant
  constant comes from a fetched `ConfigSetting*` entry or from
  `simulateTransaction`'s output; degrade visibly when a rate is unavailable.
- **Doc comments on every public item**, naming what it fetches/computes and
  which network calls it makes, if any.
- **Tests for every change**, with a regression test for every bug fix.
  Prefer deterministic mocks over live RPC in tests.
- **Keep dependencies minimal**; if you add one, `cargo deny` and `cargo audit`
  must still pass, and `Cargo.lock` must be committed.

## Commit conventions

Use [Conventional Commits](https://www.conventionalcommits.org/), one logical
change per commit:

```text
type(scope): short imperative description
```

```text
feat(rpc): add getLedgerEntries config fetch
fix(fee-calc): avoid negative refundable fee on empty footprint
docs(contributing): document the local verification gate
```

Useful scopes mirror the modules: `rpc`, `fee-calc`, `config`, `cache`, `wasm`,
`report`, `cli`, `docs`. Keep the subject imperative and under ~72 characters,
and explain **why** in the body when it is not obvious. Rebase onto
`upstream/main` rather than merging `main` into your branch.

## Open the pull request

```bash
git push -u origin <your-branch>
gh pr create --repo Stellar-Cost-Labs/soroban-cost-estimator --base main
```

The PR description should include:

1. **What changed and why** — the problem, the approach, and any trade-offs.
2. **Linked issue** — `Closes #123`.
3. **Verification** — the exact commands you ran and their results.
4. **Sample CLI output** for user-visible changes.
5. **Follow-ups** — anything intentionally left out of scope.

`main` is protected: the CI `build` check must be green (on Linux, macOS, and
Windows) before your change can merge. Request a maintainer review once CI is
green.

## PR checklist

- [ ] Linked to an issue with `Closes #…`, and scoped to its acceptance
      criteria.
- [ ] Branch rebased on the latest `upstream/main`.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
      passes.
- [ ] `cargo build --all-targets` passes.
- [ ] `cargo run --bin gen_test_wasm` then `cargo test --workspace` passes.
- [ ] Any job that failed on `Couldn't resolve host: index.crates.io` has been
      re-run, since that is a runner network failure rather than a code
      failure.
- [ ] `cargo test --test mock_rpc_tests --test cli_tests` passes.
- [ ] Tests added or updated; bug fix includes a regression test.
- [ ] No new `unwrap()`/`expect()` outside test code.
- [ ] Fee code uses integer stroop math with no new hardcoded rates.
- [ ] New errors use `AppError` and follow the message style.
- [ ] `README.md` and `docs/` updated for user-visible changes.
- [ ] Commits follow the conventional format and are logically grouped.
- [ ] PR description lists verification commands and results, including any
      check that could not be run.
- [ ] No secrets, API tokens, private RPC URLs, or personal paths included.
- [ ] CI is green and review has been requested.

## Getting help

- Open an issue for bugs, documentation gaps, and feature proposals.
- Ask implementation questions on the issue you are working on, so the answer
  stays with the work.
- Report security-sensitive issues privately — see
  [`SECURITY.md`](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/blob/main/SECURITY.md).
