# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `estimate --history` — compare the current run against up to 5 previous
  cached runs of the same contract function, rendered as a trend table
  (regressions in red, improvements in green) with a `history` array in
  `--json` output (#321).
- Resource-limit warnings — `estimate` and `estimate-all` now warn when CPU
  instructions, ledger read/write entries or bytes, or transaction size reach
  80% of the network's protocol limits, with a `warnings` array in JSON output
  (#322).
- `estimate-all --format csv` — RFC 4180-compliant CSV export with one row per
  function (`Function,Status,CPU_Instructions,...`, #324).
- `estimate-all --format markdown` — GitHub-flavored Markdown summary table,
  fee statistics, and collapsible per-function detail sections for CI PR
  comments (#325).
- `config diff --against-previous` — diff the two most recent on-disk
  snapshots against each other without contacting the live network. Works
  offline, honors `--summary` and `--json`, and errors with the snapshot count
  when fewer than two exist for the network.
- `config snapshot --retain <N>` — keep only the N most recent snapshots for a
  network, pruning older ones after the new snapshot is safely on disk.
- `config snapshot prune --older-than <DAYS>` — delete snapshots recorded more
  than D days ago. Pure file I/O, so it is safe offline and in a cron job.
- `cache list` — the read-only cache listing now accepts the same filters as
  `cache query` (`--network`, `--fn`, `--wasm-hash`, `--since`, `--to`, plus a
  new `--older-than` age filter) and is paginated with `--limit` (default 50,
  capped at 1000) and `--offset`. Without `--network` it lists entries from
  every network.
- `cache delete` — remove cached estimates by network, function, WASM hash,
  time range or age instead of clearing a whole network. Supports `--dry-run`
  (report the count, change nothing), an interactive confirmation prompt
  (skippable with `--yes`) and `--all` for an explicit whole-cache removal. A
  filter is required otherwise. Selection and deletion run in one transaction.
- Config diffs now model setting **presence** explicitly: added, removed and
  unrecognized settings are reported distinctly from numeric value changes,
  and a missing setting is never treated as a zero value.
- Unrecognized top-level fields in a stored snapshot are captured and
  surfaced in `config diff` instead of being ignored silently.
- Snapshot listings and "latest snapshot" selection are ordered by the
  timestamp embedded in the file name, so snapshots taken with different UTC
  offsets are ordered chronologically.

### Fixed

- The CLI no longer overflows the stack on Windows. `clap`'s derive macros
  build the whole command tree on the stack, which needs close to a megabyte
  in an unoptimized build, while the main thread of a Windows process is
  capped at 1 MiB by the linker. Every invocation — even `--help` — aborted
  with a stack overflow. The CLI now runs on a dedicated thread with an 8 MiB
  stack; output and exit codes are unchanged.
- `cache delete --all` can no longer be combined with a filter. `--all` means
  "every cached estimate on every network", and a stray `--network` used to
  silently narrow it; the combination is now rejected and nothing is deleted.
- Snapshot retention never deletes the newest snapshot for a network, however
  old it is or however small `--retain` is, so a `config diff` always has a
  pair left to compare.
- `webpki-roots`, `webpki-root-certs` and `rustls-webpki` bumped to their
  patched releases, which drop the distrusted `emSign ECC Root CA - G3`
  certificate (RUSTSEC-2026-0381/0382/0383) and clear the `cargo deny`
  advisory check.

### Added

- `--timeout` global flag — configurable HTTP request timeout for RPC calls
  in seconds (default 30).
- `--max-retries` global flag — configurable maximum number of retry attempts
  for transient RPC failures (default 3). 0 disables retries, so CI runs can
  opt into aggressive retries while interactive use can fail fast.
- `config diff --summary` — print a single-line summary
  (`X pricing changes, Y non-pricing changes`) instead of the full diff, for CI
  status lines. Exit code and auto-save side effects are unchanged.
- `CHANGELOG.md` following the Keep a Changelog format.
- `docs/migration.md` — migration guide for users coming from
  `stellar contract invoke --cost`.
- End-to-end integration tests in `tests/cli_tests.rs` covering every CLI
  command, its flags, and its offline error paths.
- CI build matrix running fmt, clippy, build, and tests on Linux, macOS, and
  Windows for cross-platform compatibility.
- `estimate-all` prints a fee/CPU **distribution box** (min/max/mean/median/
  standard deviation of fees and min/max/mean CPU instructions) and exposes the
  same statistics as a `fee_distribution` object in `--json` output.
- Report headers show `Simulated at ledger sequence: <n>` and JSON reports
  serialize the ledger as `ledger_sequence`.

### Changed

- `cache list` now lists entries from **every** network unless `--network` is
  given (it used to default to `testnet` only), matching `cache query`. The
  `--json` output is unchanged: a JSON array of cached-estimate records.
- `estimate-all --json` now emits an object (`{ "functions": [...],
  "fee_distribution": { ... } }`) instead of a bare array; the per-function
  records moved under the `functions` key.

### Fixed

- Restore the `cache stats` subcommand wiring that a CLI refactor merge
  dropped, leaving `cmd_cache_stats` unreachable dead code.
- WASM function signatures are now resolved against the *defined* function
  index space, so export arities stay correct for modules that import
  functions.
- Malformed `contractspecv0` / `contractmetav0` custom sections now fail with
  the section name in the error instead of being silently discarded.
- Invalid WebAssembly input reports `not a valid WebAssembly binary` instead
  of a bare validator message.
- Restore the WebSocket RPC client (`rpc::ws`) after the merge of `main`
  dropped its `AppError` variants and `resolve_ws_endpoint`, which broke
  compilation and red-flagged every CI job.

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
