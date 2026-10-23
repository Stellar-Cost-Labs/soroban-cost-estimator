# Contributing to Soroban Cost Estimator

Thanks for taking the time to contribute! This guide walks you from a fresh
clone to a green pull request. It covers the local toolchain, every command CI
runs, the repository's coding standards, and a checklist to run through before
you request review.

If anything here is wrong or missing, please open an issue — improving this
guide is itself a welcome contribution.

## Contents

- [Before you start](#before-you-start)
- [Prerequisites](#prerequisites)
- [Step-by-step development setup](#step-by-step-development-setup)
- [Running tests and benchmarks](#running-tests-and-benchmarks)
- [Code style and standards](#code-style-and-standards)
- [Commit conventions](#commit-conventions)
- [Pull request checklist](#pull-request-checklist)
- [Project structure](#project-structure)
- [Getting help](#getting-help)
- [License](#license)

## Before you start

1. Look through the [open issues](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues).
   Every issue carries a Summary, Acceptance Criteria, and Tech Stack, so it is
   clear what "done" means before you write code.
2. Comment on the issue you want to work on and wait for a maintainer to assign
   it to you. This avoids duplicated effort — especially during Drips Stellar
   Wave sprints, where several contributors may be watching the same issue.
3. Keep the change focused on the issue's acceptance criteria. Unrelated
   refactors, formatting-only churn, and drive-by dependency bumps make review
   slower and are usually asked to be split out.

## Prerequisites

| Tool | Version | Why |
|------|---------|-----|
| [git](https://git-scm.com/) | 2.30+ | Version control, rebases, forks |
| [Rust](https://www.rust-lang.org/tools/install) | **1.85+** | The crate is edition 2024 (`rust-version = "1.85"`) |
| `rustfmt` | Matches your toolchain | Formatting gate (`cargo fmt`) |
| `clippy` | Matches your toolchain | Lint gate (`cargo clippy`) |
| [GitHub CLI](https://cli.github.com/) (`gh`) | Optional | Opening issues/PRs and checking CI from the terminal |
| `wasm32v1-none` target | Optional | Only needed to **build** Soroban contracts; the estimator consumes pre-built `.wasm` files |

The quickest way to get a matching toolchain, formatter, and linter is
[rustup](https://rustup.rs/):

```bash
rustup toolchain install stable --component rustfmt clippy
rustup default stable
rustc --version   # must report 1.85.0 or newer
```

No Rust installed yet? Install rustup first:

```bash
# Linux / macOS
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows: download and run rustup-init.exe from https://rustup.rs/
```

You do **not** need a database, Docker, or a local Stellar node. Tests use
mock RPC servers; the only time real network access is required is when you
run the CLI against testnet/mainnet yourself.

## Step-by-step development setup

### 1. Fork and clone

Fork the repository on GitHub, then clone your fork (replace `<your-user>`):

```bash
git clone https://github.com/<your-user>/soroban-cost-estimator.git
cd soroban-cost-estimator
```

Add the canonical repository as `upstream` so you can rebase onto the latest
`main`:

```bash
git remote add upstream https://github.com/Stellar-Cost-Labs/soroban-cost-estimator.git
git fetch upstream
```

### 2. Create a topic branch

Branch off the latest `main`, using a descriptive `type/short-slug` name:

```bash
git checkout main
git pull --ff-only upstream main
git checkout -b docs/contributing-improvements
```

### 3. Build and generate fixtures

```bash
# Compile the CLI, the library, and all test/bench targets
cargo build --all-targets

# Generate the test WASM fixtures (tests/fixtures/minimal.wasm and zeroarg.wasm)
cargo run --bin gen_test_wasm
```

The fixture generator is a small in-repo binary that writes hand-crafted WASM
modules. CI runs it before the test suites, so run it before `cargo test` too.
The committed fixtures mean most tests work without it, but regenerating keeps
your tree identical to CI's.

### 4. Make your change

Keep commits small and scoped. When you touch behaviour, add or update the test
that proves it (unit test next to the module, or an integration test under
`tests/`).

### 5. Run the full local gate

See [Running tests and benchmarks](#running-tests-and-benchmarks) and the
[PR checklist](#pull-request-checklist). Run the complete gate again after your
last edit — a passing run before a final tweak does not count.

### 6. Push and open the pull request

```bash
git push -u origin <your-branch>
gh pr create --repo Stellar-Cost-Labs/soroban-cost-estimator \
  --fill --base main
```

In the PR description, link the issue with `Closes #123`, summarize the change,
and list the commands you ran plus their results. `--fill` only fills the title
and body from your commits; replace it with a real summary before requesting
review.

## Running tests and benchmarks

CI runs the commands below on Linux, macOS, and Windows (see
[`.github/workflows/ci.yml`](.github/workflows/ci.yml)). Running them locally in
the same order reproduces the `build` check.

### Format, lint, and build

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --all-targets
```

`clippy::all` and `clippy::pedantic` are denied in `Cargo.toml`, so warning-free
`clippy` output is required. `cargo fmt` without `--check` fixes formatting in
place.

### Generate fixtures, then test

```bash
cargo run --bin gen_test_wasm
cargo test --workspace
```

Useful subsets while iterating:

```bash
# Unit tests only (module-level #[cfg(test)] tests)
cargo test --lib

# One integration test file
cargo test --test cli_tests

# The suites CI's dedicated integration-tests job runs
cargo test --test mock_rpc_tests --test cli_tests

# One test by name (substring match)
cargo test --test fee_calc_tests refundable

# Doctests in public documentation
cargo test --doc
```

Integration test files live in `tests/` and cover the cache, CLI, config diff
and history, error message style, fee calculation (including property tests),
RPC failover and mocks, the WASM parser, report snapshots, and user config.
Snapshot tests use [`insta`](https://insta.rs/); if a snapshot changes, review
the diff with `cargo insta review` (install it with
`cargo install cargo-insta`) and commit the updated `tests/snapshots/*.snap`
file only when the new output is correct.

### Benchmarks

Benchmarks use [Criterion](https://bheisler.github.io/criterion.rs/book/) and
are not part of the pass/fail CI gate, but compile errors in them still break
`cargo build --all-targets`.

```bash
# Compile every benchmark without running it (fast sanity check)
cargo bench --no-run

# Run all benchmarks
cargo bench

# Run a single benchmark target
cargo bench --bench wasm_parse
cargo bench --bench cache_lookup
cargo bench --bench wasm_benchmark
```

Criterion writes HTML reports under `target/criterion/`. Add `-- --quick` to a
benchmark invocation while iterating.

### Optional repository-wide checks

```bash
cargo audit     # rustsec advisory scan (CI job: audit)
cargo deny check # licenses, advisories, sources, bans (CI job: deny)
cargo tarpaulin --workspace --all-features --out lcov  # coverage (CI job: coverage)
```

These need the corresponding cargo subcommand installed
(`cargo install cargo-audit cargo-deny cargo-tarpaulin`). They are welcome
before opening a PR but are not required to reproduce the core `build` gate.

### Transient crates.io failures in CI

A job that stops before compiling with repeated
`warning: spurious network error […] Couldn't resolve host name (Could not resolve host: index.crates.io)`
is a GitHub runner DNS failure, not a problem with your change. Re-run the
failed jobs from the PR's **Checks** tab (or `gh run rerun <run-id> --failed`);
the next attempt normally resolves the registry and runs the real gate.

## Code style and standards

### Rust style

- Format with `rustfmt` (`cargo fmt --all`) and keep `cargo fmt --check` clean.
- Run `clippy` with `--all-targets --all-features`; both `clippy::all` and
  `clippy::pedantic` are denied. Fix the cause instead of adding `#[allow]`
  attributes. If an allow is genuinely warranted, scope it as narrowly as
  possible and explain why in a comment.

### Error handling: no `unwrap()` or `expect()` in production code

Outside `#[cfg(test)]` modules and `tests/`, fallible operations must return
`Result` and propagate failures through the crate's `AppError` enum in
`src/error.rs`. That covers RPC calls, XDR decoding, file I/O, parsing, and
anything else that can fail.

```rust
// Good: context is added at the boundary, and the caller decides what to do.
let bytes = std::fs::read(&path)
    .map_err(|e| AppError::Io(format!("failed to read {}: {e}", path.display())))?;
```

- Prefer `?` over manual `match` when propagating.
- Add context describing **what failed**; do not leak internal prefixes.
- Error messages follow the house style described on `AppError`: lower-case
  first word unless it is an acronym (`WASM`, `RPC`, `XDR`, `XLM`, `JSON`,
  `HTTP`, `WebSocket`), lead with what failed, and end with no trailing period.
  `tests/error_tests.rs` enforces this rendering, so a new variant must comply.

### Fee math: integers only

Stroops are integers and **fee math must never touch floating point** — a float
in the fee path is a rounding bug waiting to happen.

- Use `i64`/`u64` stroop arithmetic throughout `src/report/`.
- Express percentage margins with basis-points-style integer math
  (e.g. `amount * bps / 10_000`), not `as f64`.
- Guard multiplications against overflow (`checked_mul`/`checked_add`) and
  return a descriptive `AppError` instead of panicking.
- Floating point is fine for presentation-only formatting if the codebase
  already does it there, but never for a value that feeds a fee calculation.

### No hardcoded fee constants

Every fee-relevant rate must come from a fetched `ConfigSetting*` ledger entry
or from `simulateTransaction`'s own output. Do not bake
stroops-per-resource-unit numbers into the source. If a rate cannot be fetched,
degrade visibly (warn and zero only that rate) rather than inventing a default.

### Documentation

- Every public item (function, type, module) gets a doc comment saying what it
  does — and, for anything network-facing, which RPC methods it calls.
- Update the relevant file under `docs/` when you change user-visible behaviour
  (commands, flags, output, errors), and update `README.md` when you add or
  change a command or flag.
- Keep examples runnable and consistent with the actual CLI help output.

### Tests

- Add a regression test for every bug fix, and tests for new behaviour.
- Prefer deterministic tests and mock RPC responses (`wiremock`) over live
  network calls; CI must not depend on testnet availability.
- Do not weaken or delete an assertion to make a test pass. If an assertion is
  wrong, explain why in the PR.
- Integer fee logic has property tests (`tests/fee_calc_proptest.rs`); extend
  them when you add a new arithmetic path.

### Dependencies

- Avoid adding a dependency when the standard library or an existing crate
  covers the need.
- If you do add one, pick a maintained crate, keep the version pinned
  consistently with the rest of `Cargo.toml`, and make sure
  `cargo deny check` and `cargo audit` still pass. CI runs both.
- Commit `Cargo.lock` alongside `Cargo.toml`.

## Commit conventions

Use [Conventional Commits](https://www.conventionalcommits.org/), one logical
change per commit:

```text
type(scope): short imperative description
```

Common types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`,
`ci`, `chore`. Useful scopes mirror the module tree: `rpc`, `fee-calc`,
`config`, `cache`, `wasm`, `report`, `cli`, `docs`.

Examples from this repository:

```text
feat(rpc): add getLedgerEntries config fetch
fix(fee-calc): avoid negative refundable fee on empty footprint
docs(contributing): document the local verification gate
test(cache): cover legacy JSON import migration
```

Rules of thumb:

- Subject in the imperative mood, no trailing period, roughly 72 characters.
- Explain **why** in the body when the change is not obvious.
- Rebase onto the latest `upstream/main` and resolve conflicts locally; do not
  merge `main` into your topic branch unless the maintainer asks for it.
- Do not commit generated build output, editor files, credentials, or
  machine-specific config.

## Pull request checklist

Run through this before requesting review. A PR that ticks every box is much
faster to merge.

- [ ] The PR is linked to an issue (`Closes #123`) and stays within its
      acceptance criteria.
- [ ] The branch is rebased on the latest `upstream/main`.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
      passes.
- [ ] `cargo build --all-targets` passes.
- [ ] `cargo run --bin gen_test_wasm` has been run, and
      `cargo test --workspace` passes.
- [ ] Any CI job that failed on `Couldn't resolve host: index.crates.io` has
      been re-run, since that is a runner network failure rather than a code
      failure.
- [ ] `cargo test --test mock_rpc_tests --test cli_tests` passes.
- [ ] New or changed behaviour has tests, and bug fixes have regression tests.
- [ ] No `unwrap()` or `expect()` was added outside `#[cfg(test)]`/`tests/`.
- [ ] Fee-affecting code uses integer stroop math only, with no new hardcoded
      rates.
- [ ] New errors use `AppError` and follow the message style.
- [ ] User-facing docs (`README.md`, `docs/`) are updated if flags, commands,
      output, or errors changed.
- [ ] Commits follow the conventional format and are squashed into logical
      units.
- [ ] The PR description summarizes the change, links the issue, and lists the
      verification commands and results (including any check that could not be
      run, and why).
- [ ] No secrets, tokens, private RPC URLs, or personal paths are committed.
- [ ] CI is green on all three operating systems, and a maintainer has been
      requested for review.

### Writing the PR description

Include:

1. **What changed and why** — the problem, the approach, and any trade-off.
2. **Linked issue** — `Closes #123`.
3. **How it was verified** — the exact commands you ran and their results;
   mention tests added.
4. **Screenshots or sample output** for CLI-facing changes.
5. **Follow-ups** — anything intentionally left out of scope.

## Project structure

```text
src/
  main.rs              # Binary entry point
  lib.rs               # Library crate (re-exports all modules)
  cli.rs               # Clap command definitions and dispatch
  config.rs            # config.toml loading and defaults
  error.rs             # Single AppError enum and message style
  paths.rs             # Data/config directory resolution
  cache.rs             # SQLite estimate cache, quotas, export
  interactive.rs       # Confirmation prompts for destructive commands
  wasm/                # WASM loading, validation, function enumeration
  rpc/                 # RPC client, simulateTransaction, retry, WebSocket
  config_snapshot/     # Config model, snapshot store, diff, history
  report/              # Cost report formatting, fee calculation, diffs
  xdr_helper.rs        # Stellar XDR encode/decode helpers
  bin/gen_test_wasm.rs # Fixture generator (developer tool)
tests/
  fixtures/            # Test WASM files and deployment notes
  snapshots/           # insta snapshot files
  common/              # Shared test helpers
  *_tests.rs           # Unit-adjacent and integration test suites
benches/               # Criterion benchmarks (WASM parse, cache, end-to-end)
docs/                  # GitBook documentation (also in docs/contributing.md)
fuzz/                  # Excluded from the workspace; run manually
```

## Getting help

- Open an issue for bugs, documentation gaps, and feature proposals.
- Ask implementation questions on the issue you are working on so the answer
  stays with the work.
- Report security-sensitive problems privately — see [SECURITY.md](SECURITY.md).

## License

By contributing, you agree that your contributions will be licensed under the
[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) license, at the project's
option.
