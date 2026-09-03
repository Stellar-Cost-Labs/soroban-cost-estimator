# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Commands

- Cache warm-up command and WASM-change invalidation (#168).
- `--cache-ttl` flag to reuse fresh cached estimates without re-simulating (#189).

#### Core

- Cache schema-version field and automatic migration path for cache entries
  (#185).
- Cache integrity verification to detect corrupted or tampered entries (#144).
- Config setting change history tracking (#181).
- Automatic snapshot on network upgrade detection (#143).
- Snapshot history with timestamp-based navigation (#146, #150, #153).
- Formatter trait implementations for pluggable report output formats (#180).
- Structured tracing across all modules for improved diagnostics.
- `#[must_use]` annotations on pure value-returning functions (#145).
- Enhanced error handling for file-not-found scenarios.
- Extracted `fetch_config_snapshot` and `print_stale_estimates` helpers for
  better main-loop readability (#151).

#### Project

- Performance benchmarks for WASM parsing (#184).
- Snapshot testing for cost report formatters using `insta` (#183).
- Concurrency tests for cache save/load access (#182).
- Property-based tests for fee calculator (#152).
- Edge-case coverage for `xlm_to_stroops` (#149).
- Expanded `parse_interval_secs` edge-case coverage (#147).
- Code-coverage reporting with `cargo-tarpaulin` (#177).
- Dependency audit and vulnerability scanning CI job (#172).
- Comprehensive command reference documentation page (#162).
- Architecture overview document (#160).
- Troubleshooting guide (#158).
- Migration guide (`docs/migration.md`) for users coming from
  `stellar contract invoke --cost`.
- `--timeout` global flag — configurable HTTP request timeout for RPC calls
  in seconds (default 30).
- `config diff --summary` — print a single-line summary
  (`X pricing changes, Y non-pricing changes`) instead of the full diff, for CI
  status lines. Exit code and auto-save side effects are unchanged.
- `CHANGELOG.md` following the Keep a Changelog format.
- `docs/migration.md` — migration guide for users coming from
  `stellar contract invoke --cost`.
- End-to-end integration tests covering every CLI command, its flags, and
  its offline error paths.

### Fixed

- Corrected error messages and handling for missing files.

### Changed

- Refactored main CLI loop to extract config snapshot fetching and stale-
  estimate printing into dedicated helpers (#151).
- End-to-end integration tests in `tests/cli_tests.rs` covering every CLI
  command, its flags, and its offline error paths.
- CI build matrix running fmt, clippy, build, and tests on Linux, macOS, and
  Windows for cross-platform compatibility.

## [0.1.0] - 2026-08-13

Initial release. Everything below landed in the run-up to `0.1.0`; the list is
grouped by area rather than by commit.

### Added

#### Commands

- `estimate` — simulate a single contract invocation (or a WASM upload when
  `--fn` is omitted) and print a cost report as a table or JSON.
  Flags: `--wasm`, `--network`, `--rpc-url`, `--fn`, `--id`, `--arg`, `--json`.
- `estimate-all` — enumerate every exported contract function and estimate each
  one, with spec-driven signatures decoded from the `contractspecv0` custom
  section. Flags: `--wasm`, `--network`, `--id`, `--json`.
- `config snapshot` — fetch all six `ConfigSetting*` ledger entries and save a
  timestamped snapshot under `~/.soroban-cost-estimator/snapshots/`.
  Flags: `--network`, `--out`, `--json`.
- `config diff` — diff the live network config against a stored snapshot,
  flagging pricing changes and cross-referencing cached estimates that the
  change may have made stale. Exits `1` when pricing changed.
  Flags: `--network`, `--against`.
- `watch` — poll the network config on an interval and print a diff whenever
  one appears. Flags: `--network`, `--interval` (accepts `s`/`m`/`h`/`d`
  suffixes, or a bare number of seconds).

#### Core

- Unified `AppError` enum and `AppResult<T>` alias covering I/O, RPC, XDR,
  WASM, snapshot, simulation, fee-calculation, and JSON failures — no
  `unwrap()`/`expect()` outside tests.
- WASM loader that validates the module and enumerates exported functions,
  with typed parameters decoded from the contract spec when present.
- Minimal JSON-RPC 2.0 client with well-known endpoint resolution for
  `testnet`, `mainnet`, and `futurenet`, overridable via `--rpc-url`.
- `simulateTransaction` wrapper that parses resources from both legacy `cost`
  responses and modern `transactionData` XDR.
- Fee calculator deriving the non-refundable and refundable components
  independently from live network config rates, with `(units * rate) / scale`
  math that preserves precision.
- Cost report rendering in both table and JSON form.
- Typed config-snapshot model for the six `ConfigSetting` types, a file-backed
  snapshot store, and a field-level diff that flags pricing-relevant changes.
- Estimate cache under `~/.soroban-cost-estimator/cache/`, keyed by WASM hash,
  function name, and argument hash, used to detect stale estimates.
- XDR decode helpers and a simulation transaction envelope builder.
- Graceful shutdown for `watch` on `SIGINT`/`SIGTERM` — the in-flight poll is
  cancelled rather than writing a partial snapshot.
- Loud failure when a simulation returns neither cost data nor a latest
  ledger, instead of silently printing an all-zero report.
- A warning naming any `ConfigSetting*` fee-rate source that could not be
  fetched, so an understated non-refundable fee is never silent.
- Real Soroban `increment` contract fixture plus a WASM generator binary
  (`gen_test_wasm`).

#### Project

- Integration tests for the CLI, cache, config diff, and WASM parser.
- CI workflow gating on `cargo fmt`, `cargo clippy -D warnings`, build, and
  `cargo test`.
- GitBook documentation site: introduction, installation, concepts (resource
  fees, config drift, caching), a command reference for all five commands,
  architecture, verification, contributing, and FAQ.
- Dual MIT / Apache-2.0 licensing.
- README with quick start, the differentiator writeup, live testnet
  verification numbers, and CI gate documentation.

### Fixed

- Fee calculation derived the refundable component by subtraction; it is now
  derived independently from config rates.
- Repository metadata pointed at the wrong org.
- Relative documentation links that did not resolve on GitBook.
- Infallible `expect()` in the `parse_arg_scval` fallback path replaced with
  a real fallible path.

[Unreleased]: https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/releases/tag/v0.1.0

<!-- Changelog generated from git log. See also: docs/migration.md -->
