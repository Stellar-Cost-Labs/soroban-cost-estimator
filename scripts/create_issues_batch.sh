#!/usr/bin/env bash
# Batch-create 125 issues for soroban-cost-estimator
# Compliant with Drips Wave Issue Creation Framework:
# https://www.drips.network/blog/posts/creating-meaningful-issues
set -euo pipefail

REPO="${1:-Stellar-Cost-Labs/soroban-cost-estimator}"

if ! command -v gh >/dev/null 2>&1; then
    echo "error: gh CLI not found — install and authenticate it first (gh auth login)" >&2
    exit 1
fi

# Ensure labels exist
gh label create "Stellar Wave" --repo "$REPO" --color 1d76db --description "Drips Stellar Wave sprint" --force >/dev/null 2>&1 || true
gh label create "complexity: trivial" --repo "$REPO" --color 0e8a16 --description "100 points" --force >/dev/null 2>&1 || true
gh label create "complexity: medium" --repo "$REPO" --color fbca04 --description "150 points" --force >/dev/null 2>&1 || true
gh label create "complexity: high" --repo "$REPO" --color d93f0b --description "200 points" --force >/dev/null 2>&1 || true

create_issue() {
  local title="$1"
  local labels="$2"
  local body="$3"
  gh issue create --repo "$REPO" --title "$title" --label "$labels" --body "$body"
  echo "Created: $title"
}


# ═══════════════════════════════════════════════════════════════════════════════
# CLI FEATURES & DEVELOPER EXPERIENCE (#28–52)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "feat(cli): add `--verbose` flag for debug output" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `--verbose` flag that prints debug-level output including RPC request/response details, XDR decode steps, and timing information.

## Background
When troubleshooting unexpected fee estimates or network simulation errors, developers currently have no visibility into the raw JSON-RPC requests, base64 XDR payloads, or execution latency. A global `--verbose` flag enables transparent inspection without polluting stdout in machine-readable modes.

## Acceptance criteria
- [ ] `--verbose` / `-v` global flag added to `Cli`
- [ ] Prints endpoint URL, HTTP method, payload sizes, elapsed duration, and XDR decode steps to stderr
- [ ] Output is suppressed when `--verbose` is not passed
- [ ] Stderr output does not interfere with `--json` stdout
- [ ] Tests cover `--verbose` flag presence on all subcommands

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cli-verbose`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/client.rs`

Add `#[arg(long, short, global = true)] verbose: bool` in `src/cli.rs`. Pass `verbose` flag to RPC client and execution handlers in `src/main.rs`. Log timing and request meta to `eprintln!`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Structured file logging framework
- Tracing subscriber integration (see #145)
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--output` flag to write results to a file" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add an `--output <path>` flag to `estimate`, `estimate-all`, and `config diff` to write results directly to a specified file.

## Background
In automated CI workflows or benchmark pipelines, capturing stdout can accidentally mix in terminal escape codes or debug notices. An explicit `--output` flag writes the rendered table or JSON output to the specified filesystem path, creating parent directories if needed.

## Acceptance criteria
- [ ] `--output <path>` / `-o <path>` flag available for `estimate`, `estimate-all`, and `config diff`
- [ ] Automatically creates parent directories if they do not exist
- [ ] Returns a descriptive error if the path is invalid or unwritable
- [ ] Works with both table and JSON outputs

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cli-output-flag`

Key files:
- `src/cli.rs`
- `src/main.rs`

Add `#[arg(long, short)] output: Option<std::path::PathBuf>` to command variants. In `src/main.rs`, format the report string and write to file with `std::fs::write` after `std::fs::create_dir_all`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Streaming output to remote endpoints
- Appending to existing files
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add shell completion generation subcommand" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `completions <shell>` subcommand that generates autocompletion scripts for Bash, Zsh, Fish, PowerShell, and Elvish.

## Background
Soroban Cost Estimator features multiple subcommands and arguments (`estimate`, `estimate-all`, `config snapshot`, `config diff`, `watch`). Generating shell completion scripts enables seamless tab-completion in developer terminals.

## Acceptance criteria
- [ ] `soroban-cost-estimator completions <SHELL>` generates completion scripts to stdout
- [ ] Supported shells: bash, zsh, fish, powershell, elvish
- [ ] Auto-completes subcommands, network names (`testnet`, `mainnet`, `futurenet`, `local`), and flags
- [ ] Unit test asserts valid generation without panics

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/shell-completions`

Key files:
- `Cargo.toml`
- `src/cli.rs`
- `src/main.rs`

Add `clap_complete` to `Cargo.toml`. Add `Completions { shell: clap_complete::Shell }` variant to `Commands` in `src/cli.rs`. Implement `clap_complete::generate` in `src/main.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Dynamic completion of live remote contract IDs
- OS package manager integration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--no-cache` flag to skip cache reads and writes" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `--no-cache` flag to `estimate` and `estimate-all` to force fresh simulations and bypass local cache reads.

## Background
Estimator caches simulation results based on contract hash, function name, and arguments. When benchmarking network changes or debugging transient state differences, users need a way to bypass cache lookup without deleting `~/.soroban-cost-estimator/cache`.

## Acceptance criteria
- [ ] `--no-cache` flag added to `estimate` and `estimate-all` commands
- [ ] When `--no-cache` is passed, cache lookup is skipped and live RPC simulation is executed
- [ ] When `--no-cache` is passed, fresh results are returned without reading stale disk records
- [ ] Tests verify cache is ignored when flag is present

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/no-cache-flag`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `#[arg(long)] no_cache: bool` to `Estimate` and `EstimateAll` in `src/cli.rs`. In `src/main.rs`, skip `load_estimate` if `no_cache` is true.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Deleting existing cache entries
- Configuring cache eviction policies
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--quiet` flag to suppress non-essential output" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `--quiet` / `-q` flag to suppress progress spinners, info banners, and non-error notices, outputting only the final result or error.

## Background
In automated pipelines, cron jobs, and scripts, extra banner logs and progress indicators can clutter logs or complicate text filtering. A `--quiet` flag guarantees only the raw table, JSON result, or critical failure messages are emitted.

## Acceptance criteria
- [ ] `--quiet` / `-q` global flag added to `Cli`
- [ ] Suppresses all progress bars, spinners, informational headers, and non-error warnings
- [ ] In case of errors, error message is still printed to stderr with non-zero exit code
- [ ] Works seamlessly with both `--json` and table outputs

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/quiet-flag`

Key files:
- `src/cli.rs`
- `src/main.rs`

Add `#[arg(long, short, global = true)] quiet: bool` in `src/cli.rs`. Check `cli.quiet` in `main.rs` before printing spinners, banners, or progress updates.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Silent exit without error messages on failure
- Log levels configuration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--version` flag with extended build metadata" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Enhance `--version` output to display crate version, git commit hash, build timestamp, and target architecture.

## Background
When users report bugs or when tracking binary deployments across multiple machines, knowing the exact git commit and build date is essential for diagnosing discrepancies in simulation behavior.

## Acceptance criteria
- [ ] `--version` / `-V` prints version, git SHA (or 'clean' release tag), rustc version, and target triple
- [ ] Outputs format: `soroban-cost-estimator 0.1.0 (commit: <sha> built: <date> target: <triple>)`
- [ ] Fallback gracefully if git metadata is not available during build
- [ ] Clippy and formatting clean

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/version-metadata`

Key files:
- `Cargo.toml`
- `build.rs`
- `src/cli.rs`
- `src/main.rs`

Create a `build.rs` script that captures `VERGEN_GIT_SHA` or read `git rev-parse HEAD` with fallback to `CARGO_PKG_VERSION`. Set clap `version(env!("BUILD_VERSION"))` in `src/cli.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Auto-updating binary over network
- Release channel switching
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add config file support (`config.toml`)" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Support loading user configuration from `~/.config/soroban-cost-estimator/config.toml` for default network, RPC endpoints, and output preferences.

## Background
Developers repeatedly pass `--network testnet --rpc-url <custom-rpc>` on every command. Supporting a standard TOML config file allows teams to define custom RPC endpoints and default flags once, while retaining CLI flag override precedence.

## Acceptance criteria
- [ ] Loads config from `$XDG_CONFIG_HOME/soroban-cost-estimator/config.toml` or `~/.soroban-cost-estimator/config.toml`
- [ ] Supports keys: `default_network`, `rpc_urls` (map of network -> url), `format`, `timeout_secs`
- [ ] CLI arguments always override configuration file values
- [ ] `--config <path>` flag allows overriding the config file location
- [ ] Gracefully creates or ignores missing config file without error

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-file-support`

Key files:
- `Cargo.toml`
- `src/cli.rs`
- `src/main.rs`
- `src/error.rs`

Add `toml` and `serde` support. Create `src/config.rs` with `UserConfig` struct. Merge `UserConfig` with clap parsed options before dispatching commands.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Interactive config wizard in terminal
- Cloud synced profiles
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config snapshot list` subcommand" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config snapshot list` subcommand that lists all saved network configuration snapshots with timestamps and ledger numbers.

## Background
Users accumulate configuration snapshots over time but have to manually inspect the filesystem (`~/.soroban-cost-estimator/snapshots`) to see what snapshots exist. A list subcommand provides a structured overview of historical network states.

## Acceptance criteria
- [ ] `config snapshot list` lists all saved snapshots for the default network
- [ ] `--network <name>` filters snapshots for a specific network (or `--all` for all networks)
- [ ] Output shows table with Filename, Network, Timestamp, Ledger Sequence, and Protocol Version
- [ ] `--json` outputs a structured JSON array of snapshot metadata
- [ ] Displays friendly message if no snapshots are found

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/snapshot-list`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/store.rs`

Add `List { #[arg(long)] network: Option<String>, #[arg(long)] json: bool }` to `ConfigAction` in `src/cli.rs`. Implement `list_snapshots_metadata` in `src/config_snapshot/store.rs` that reads and deserializes headers.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Diffing multiple snapshots in the list view
- Deleting snapshots from list view
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config snapshot delete` subcommand" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config snapshot delete` subcommand to delete a specific snapshot or purge snapshots older than N days.

## Background
As snapshots accumulate over months of development, users need an easy way to clean up stale snapshot files without manual directory browsing.

## Acceptance criteria
- [ ] `config snapshot delete <filename>` deletes a specific snapshot file
- [ ] `config snapshot delete --older-than <days>` deletes snapshots older than specified days
- [ ] `--dry-run` shows which files would be removed without deleting them
- [ ] `--yes` / `-y` skips confirmation prompts in non-interactive sessions
- [ ] Returns error if specified snapshot does not exist

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/snapshot-delete`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/store.rs`

Add `Delete` variant to `ConfigAction`. Implement deletion helper in `store.rs` using `std::fs::remove_file` with date parsing on filename timestamps.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Cloud backup before deletion
- Trash bin recovery
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config snapshot diff` for offline snapshot comparison" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `config snapshot diff <snapshot_a> <snapshot_b>` to compare two saved snapshot files completely offline without making network calls.

## Background
Currently `config diff` fetches the live network configuration and compares it with the latest saved snapshot. Developers often need to compare two historical snapshots (e.g. Protocol 20 vs Protocol 21 snapshots) offline without requiring network access.

## Acceptance criteria
- [ ] `config snapshot diff <file_a> <file_b>` loads both snapshot JSON files from disk and computes diff
- [ ] Outputs the standard formatted diff table highlighting pricing changes
- [ ] Supports `--json` flag for machine-readable diff output
- [ ] Exits with code 0 (no pricing changes) or 1 (pricing changes detected) matching `config diff` semantics
- [ ] Clear error message if either snapshot file cannot be read or parsed

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/snapshot-diff-offline`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/diff.rs`
- `src/config_snapshot/store.rs`

Add `DiffFiles { file_a: PathBuf, file_b: PathBuf, #[arg(long)] json: bool }` to `ConfigAction`. Reuse `diff_snapshots(&old, &new)` from `src/config_snapshot/diff.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- 3-way merge diffing
- Interactive TUI diff viewer
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--compare` flag to `estimate` showing cost delta vs previous estimate" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `--compare` flag to `estimate` that looks up the previously cached estimate for the same function and displays cost differences.

## Background
When optimizing smart contract functions, developers iterate on code changes and run `estimate`. Having an immediate side-by-side delta showing `+12,400 CPU (-5.2%)` and `+100 stroops` helps verify whether an optimization actually reduced execution cost.

## Acceptance criteria
- [ ] `--compare` flag added to `estimate`
- [ ] If a cached previous estimate exists, displays a delta column/section with absolute and percentage change for CPU, Memory, IO, and Fee
- [ ] If no previous estimate exists, displays notice: 'No previous estimate found for comparison'
- [ ] Supports `--json` mode with `previous_estimate` and `delta` objects
- [ ] Unit tests verify delta math for positive, negative, and zero differences

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/estimate-compare`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`
- `src/report/cost_report.rs`

Add `compare: bool` to `Estimate` args. In `src/main.rs`, load cached entry before simulation, execute live simulation, compute deltas, and render comparison table.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Tracking git commit history of estimates (see #81)
- Multi-version comparison matrices
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--format` flag supporting table, json, csv, and markdown" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Unify output formatting across all commands behind a `--format <table|json|csv|markdown>` CLI argument.

## Background
Currently `--json` is a boolean flag, and other formats (CSV, Markdown) are not natively selectable. Introducing a unified `--format` enum enables consistent formatting options across `estimate`, `estimate-all`, and `config diff`.

## Acceptance criteria
- [ ] `--format <table|json|csv|markdown>` global flag supported on `estimate`, `estimate-all`, `config diff`
- [ ] `--json` remains supported as a backward-compatible alias for `--format json`
- [ ] Table format renders terminal tables with comfy-table
- [ ] Markdown format renders GitHub-flavored markdown tables
- [ ] CSV format outputs standard RFC 4180 comma-separated records
- [ ] JSON format outputs structured JSON payload

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/format-flag`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/report/mod.rs`

Define `enum OutputFormat { Table, Json, Csv, Markdown }` in `src/cli.rs` implementing `clap::ValueEnum`. Implement format dispatchers in `src/report/`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- HTML report generation with embedded charts
- PDF export
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `estimate --diff` to compare two WASM files" "enhancement, complexity: high, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `estimate --diff --wasm <v1.wasm> --wasm-new <v2.wasm>` to simulate and compare resource usage between two WASM builds.

## Background
Before deploying a contract upgrade or refactor, developers want to know the exact cost difference between the current deployed WASM binary and their newly compiled WASM binary under identical arguments.

## Acceptance criteria
- [ ] `estimate` accepts `--wasm-new <path>` when comparing builds
- [ ] Simulates the specified function (or upload) on both WASM binaries against the same network
- [ ] Outputs side-by-side comparison table showing: WASM Size, CPU Instructions, RAM, Read/Write bytes, and Total Fee with diff indicators (+/-%).
- [ ] Supports `--json` output with `wasm_a`, `wasm_b`, and `diff` structures
- [ ] Returns error if function signature is missing in either WASM

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/estimate-diff-wasm`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/report/cost_report.rs`
- `src/rpc/simulate.rs`

Add `wasm_new: Option<PathBuf>` to `Estimate` in `src/cli.rs`. In `src/main.rs`, run two simulations, construct a `CostReportDiff` struct, and format comparative report.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Decompiling and diffing WASM assembly instructions (see #69)
- Auto-deploying the lower-cost contract
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config cache stats` subcommand" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `config cache stats` to report total cached entries, disk space consumed, and date range of cached estimates.

## Background
Users cannot currently inspect cache volume, entry count, or disk utilization without navigating `~/.soroban-cost-estimator/cache`. A stats command provides instant operational visibility.

## Acceptance criteria
- [ ] `config cache stats` displays: Total cached estimates, total disk size in KB/MB, per-network breakdown, oldest estimate timestamp, newest estimate timestamp
- [ ] `--json` outputs machine-readable statistics object
- [ ] Handles empty cache directory cleanly with 'Cache is empty (0 entries, 0 bytes)'
- [ ] Integration test checks stats output with mock entries

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-stats-cmd`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `Stats` variant under a new `CacheAction` subcommand in `src/cli.rs`. Implement `get_cache_stats()` in `src/cache.rs` that iterates cache files and aggregates counts and metadata.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Remote cache statistics aggregation
- Auto-compacting cache database
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `estimate --dry-run` to preview simulation parameters without RPC calls" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--dry-run` flag to `estimate` that parses WASM, decodes contract spec, validates arguments, constructs the transaction envelope, and prints the planned simulation payload without contacting the network.

## Background
For air-gapped environments or local contract verification, developers want to verify that argument types match the contract spec and that the transaction XDR will serialize cleanly without dispatching live network HTTP calls.

## Acceptance criteria
- [ ] `estimate --dry-run` performs WASM parsing, spec inspection, and ScVal argument construction
- [ ] Prints: Resolved RPC endpoint, Contract ID, Function name, ScVal argument breakdown, estimated transaction envelope size, and base64 transaction data
- [ ] Makes zero network requests
- [ ] Returns error immediately if argument types do not conform to contract spec

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/estimate-dry-run`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/xdr_helper.rs`

Add `#[arg(long)] dry_run: bool` in `src/cli.rs`. In `src/main.rs`, construct `SorobanTransactionData` and print planned parameters before invoking `simulate_transaction`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Local offline WASM execution engine
- Offline fee calculation without config settings
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config snapshot --auto` flag to snapshot on every estimate" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add an `--auto-snapshot` option to `estimate` and `estimate-all` that automatically saves a new config snapshot if the network configuration has changed.

## Background
Manual snapshots require running `config snapshot` separately. An automatic snapshot mode ensures that every time a developer runs an estimate, any newly detected network pricing changes are recorded without manual intervention.

## Acceptance criteria
- [ ] `--auto-snapshot` flag added to `estimate` and `estimate-all`
- [ ] After fetching simulation data, checks if network config differs from latest snapshot
- [ ] If changed, automatically writes new timestamped snapshot and informs user: 'Network configuration updated: saved snapshot <filename>'
- [ ] If unchanged, skips disk write
- [ ] Works without slowing down simulation runs

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/auto-snapshot`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/mod.rs`

Add `auto_snapshot: bool` in `src/cli.rs`. In `src/main.rs`, fetch live config, compare with latest snapshot hash/version, and call `store::save_snapshot` if modified.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automatic rollback of network configurations
- Git auto-commit of snapshots
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `estimate-all --json` with structured per-function results" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Ensure `estimate-all --json` emits a fully structured, deterministic JSON object containing contract metadata, overall summary, and per-function fee breakdowns.

## Background
Automation scripts require consistent JSON structure when evaluating all functions in a WASM contract. Currently `estimate-all` lacks a comprehensive JSON output schema with error states for functions that fail simulation.

## Acceptance criteria
- [ ] `estimate-all --json` returns JSON object with `contract_wasm_hash`, `network`, `functions` array, and `total_summary`
- [ ] Each function entry includes: `function_name`, `status` ('success' | 'failed'), `resources` (CPU, memory, read/write bytes), `fee_breakdown`, and `error_message` if failed
- [ ] Functions array is deterministically sorted by function name
- [ ] Non-zero exit code if any function fails simulation, while still outputting valid JSON
- [ ] Schema documented in docs and covered by snapshot test

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/estimate-all-json`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/report/cost_report.rs`

Create `EstimateAllReport` struct deriving `Serialize`. In `cmd_estimate_all`, collect results into `EstimateAllReport` and serialize with `serde_json::to_string_pretty`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Interactive HTML visualizer
- Streaming WebSocket events
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config diff --exit-code` flag for CI drift detection" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add explicit `--exit-code` flag to `config diff` to control whether pricing changes trigger non-zero exit codes in CI.

## Background
In CI pipelines, teams want `config diff` to fail the build (exit code 1) when network pricing changes are detected, but succeed (exit code 0) when running informative reports. Providing explicit control makes pipeline scripts more robust.

## Acceptance criteria
- [ ] `config diff` exits with 0 if no pricing changes, 1 if pricing changes detected (default behavior preserved)
- [ ] `--ignore-pricing-exit` flag added to force exit code 0 even if pricing changes occur
- [ ] `--fail-on-any-change` flag added to exit with 1 if any config setting changed, even non-pricing settings
- [ ] Documentation updated with CI exit code semantics

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-diff-exit-codes`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/diff.rs`

Add `ignore_pricing_exit: bool` and `fail_on_any_change: bool` to `ConfigAction::Diff`. In `cmd_config_diff`, evaluate exit code based on flags and `ConfigDiff::has_pricing_changes()`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom webhook callbacks on failure
- Slack alerts from CLI
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add interactive `--interactive` mode for function argument input" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add an interactive prompt mode (`estimate --interactive`) that prompts the user for function selection and parameter values based on the contract spec.

## Background
Typing `--arg key=val` for contracts with multiple parameters can be error-prone. An interactive CLI prompt displays available functions and prompts for each parameter with its expected type (e.g. 'Enter step (i64): ').

## Acceptance criteria
- [ ] `estimate --interactive` / `-i` parses the WASM contract spec
- [ ] Prompts user to select a function from the contract spec if `--fn` was not provided
- [ ] Prompts for each required parameter displaying name and expected type
- [ ] Validates user input against expected type before proceeding
- [ ] Gracefully handles `Ctrl-C` to abort prompt without panic

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/interactive-mode`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/wasm/parser.rs`

Add `#[arg(long, short = '''i''')] interactive: bool` to `Estimate`. In `src/main.rs`, if interactive is true, use `std::io::stdin()` to read lines and validate with `xdr_helper`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Full-screen TUI (Ratatui)
- Mouse support
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config cache export` to dump cache as JSON" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config cache export <file.json>` subcommand to dump all cached estimates to a single structured JSON export file.

## Background
Developers and teams working across multiple workstations need a way to back up, share, or archive cached simulation estimates without manually copying internal cache directories.

## Acceptance criteria
- [ ] `config cache export <path>` writes all cached estimates for the specified network (or all networks) to a JSON file
- [ ] Export format includes schema version, export timestamp, and list of `CachedEstimate` records
- [ ] Prints confirmation with number of exported entries and output file path
- [ ] Returns error if export destination is unwritable

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-export`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `Export { output: PathBuf, #[arg(long)] network: Option<String> }` to `CacheAction`. Implement `export_cache` in `src/cache.rs` that loads all entries and serializes to file.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Encrypted export archives
- Direct S3 export upload
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config cache import` to restore cache from JSON" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config cache import <file.json>` subcommand to restore cached simulation estimates from an export file.

## Background
Allows CI runners or team members to pre-seed their local cache with historical baseline estimates from team exports.

## Acceptance criteria
- [ ] `config cache import <path>` reads export JSON and populates local cache store
- [ ] `--merge` mode merges with existing cache entries without overwriting newer ones
- [ ] `--overwrite` mode replaces existing cache entries
- [ ] Validates schema version and rejects corrupted export files with informative error
- [ ] Prints summary: 'Imported X new entries, skipped Y existing entries'

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-import`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `Import { file: PathBuf, #[arg(long)] overwrite: bool }` to `CacheAction`. Implement `import_cache` in `src/cache.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Auto-syncing cache over peer-to-peer network
- Remote database sync
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `estimate --repeat N` for simulation benchmarking" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--repeat <N>` flag to `estimate` to run the simulation N times and report latency statistics (min, max, mean, stddev) and fee consistency.

## Background
When evaluating RPC endpoint stability and fee estimation determinism under varying network loads, developers need to run repeated simulations and measure RPC latency percentiles and resource consistency.

## Acceptance criteria
- [ ] `estimate --repeat <N>` runs simulation N times (where N >= 1, max 100)
- [ ] Prints summary table with: Iteration count, Min latency, Max latency, Mean latency, CPU Instructions (asserted identical), Total Fee (asserted identical)
- [ ] Warns if fee or resource estimates vary across repeated runs (indicating non-deterministic simulation)
- [ ] Supports `--json` output with array of run latencies and statistical metrics

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/estimate-repeat`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/simulate.rs`

Add `#[arg(long, default_value = "1")] repeat: u32` in `src/cli.rs`. In `src/main.rs`, loop simulation N times collecting `Instant::elapsed()`, calculate summary stats, and format output.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Distributed load testing
- Simulating parallel concurrent load across 1000s of threads
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `--color` flag for terminal color control" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--color <auto|always|never>` flag to control ANSI color formatting in terminal output.

## Background
Terminal colors highlight pricing increases (red) and decreases (green). However, in non-TTY environments, piping to `less`, or adhering to the `NO_COLOR` standard, users need explicit control over color formatting.

## Acceptance criteria
- [ ] `--color <auto|always|never>` global flag added to `Cli`
- [ ] Defaults to `auto` (detects if stdout is a TTY and checks `NO_COLOR` environment variable)
- [ ] `always` forces ANSI color escape codes even when redirected
- [ ] `never` strips all ANSI escape codes
- [ ] Comfy-table styling respects the color setting

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/color-flag`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/report/cost_report.rs`

Add `#[arg(long, global = true, default_value = "auto")] color: ColorChoice` using `clap::ColorChoice`. Configure comfy table colorization and colored output based on choice.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom RGB theme configuration files
- 256-color palette editor
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `config diff --summary` for concise one-line output" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `--summary` flag to `config diff` that outputs a single-line status summary suitable for notification bars or shell prompts.

## Background
When integrating `config diff` into terminal status bars (tmux, starship) or notification webhooks, full table output is too verbose. A single-line summary provides instant awareness of network drift.

## Acceptance criteria
- [ ] `config diff --summary` prints one line: 'Network config up to date (ledger <seq>)' or 'Config drift detected: X pricing changes, Y non-pricing changes'
- [ ] Exits with standard diff exit codes (0 for no pricing changes, 1 for pricing changes)
- [ ] Works with `--network` option
- [ ] Tests assert exact string formatting

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-diff-summary`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/diff.rs`

Add `#[arg(long)] summary: bool` to `ConfigAction::Diff`. In `cmd_config_diff`, if `summary` is true, count pricing vs non-pricing changes and print single line.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Desktop OS notification popups
- Webhook dispatch
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cli): add `estimate --watch` mode for continuous re-estimation on file changes" "enhancement, complexity: high, Stellar Wave" "$(cat <<'EOF'
## Summary
Add an `estimate --watch` flag that watches the WASM file for rebuilds on disk and automatically re-runs cost estimation.

## Background
During contract development in Rust, developers edit code and run `cargo build --target wasm32-unknown-unknown`. An `--watch` mode monitors the compiled WASM binary, instantly re-simulates upon modification, and prints updated cost deltas.

## Acceptance criteria
- [ ] `estimate --watch` monitors the target `.wasm` file using file modification polling or notify crate
- [ ] Re-simulates the function whenever the WASM file timestamp/hash changes
- [ ] Clears screen or prints a clean header showing timestamp and cost change compared to previous build
- [ ] Handles `Ctrl-C` (SIGINT) cleanly with exit code 0
- [ ] Ignores temporary partial writes during active compilation

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/estimate-watch`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/simulate.rs`

Add `#[arg(long)] watch: bool` to `Estimate`. In `src/main.rs`, spawn a polling loop checking WASM file SHA-256 hash or mtime, sleeping for 500ms, and invoking `simulate_transaction` on change.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automatically invoking `cargo build` on source code changes (developer runs compiler in their own terminal)
- Live hot-reloading deployed contracts
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# RPC & NETWORK RELIABILITY (#53–67)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "fix(rpc): add exponential backoff on transient RPC failures" "bug, network, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Implement exponential backoff with jitter for transient Soroban JSON-RPC failures (HTTP 429, 502, 503, 504, and network timeouts).

## Background
Soroban public RPC endpoints (especially testnet) occasionally return transient 429 rate limit errors or 503 gateway timeouts during traffic spikes. The CLI currently fails immediately without retry, breaking CI workflows and long-running watch sessions.

## Acceptance criteria
- [ ] Retries transient HTTP errors (429, 500, 502, 503, 504) and connection resets up to 3 times by default
- [ ] Implements exponential backoff: base 500ms * 2^attempt with full jitter (e.g. 500ms, 1000ms, 2000ms + random offset)
- [ ] Does NOT retry deterministic client errors (e.g. 400 Bad Request, invalid XDR)
- [ ] Honors `Retry-After` header if returned by the RPC endpoint on HTTP 429
- [ ] Logs retry attempts with duration to stderr when `--verbose` is enabled

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b fix/rpc-exponential-backoff`

Key files:
- `src/rpc/client.rs`
- `src/rpc/simulate.rs`
- `src/error.rs`

Create `with_retry<F, Fut, T, E>(operation: F) -> Result<T, E>` in `src/rpc/client.rs`. Use `tokio::time::sleep` for async delay with jitter.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Configurable custom backoff strategies via plugin
- Circuit breaker pattern across process restarts
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add configurable request timeout (`--timeout`)" "enhancement, network, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `--timeout <seconds>` flag to control HTTP connection and response timeouts for all RPC requests.

## Background
Default HTTP client timeouts can hang indefinitely on degraded networks or fail too quickly on complex smart contract simulations. Allowing users to configure the timeout ensures reliable operation in varied network conditions.

## Acceptance criteria
- [ ] `--timeout <secs>` flag added to all CLI commands making RPC calls
- [ ] Defaults to 30 seconds if unspecified
- [ ] Applies to `reqwest::ClientBuilder::timeout`
- [ ] Returns a descriptive timeout error if RPC response exceeds the duration: 'RPC request timed out after X seconds'
- [ ] Unit test asserts timeout configuration on the HTTP client

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-timeout-flag`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/client.rs`

Add `#[arg(long, global = true, default_value = "30")] timeout: u64` in `src/cli.rs`. Pass timeout duration to `RpcClient::new(endpoint, Duration::from_secs(timeout))`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Per-request dynamic adaptive timeout calculation
- Cancelling server-side execution
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add HTTP connection pooling with keep-alive" "enhancement, performance, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Configure reqwest client connection pooling and TCP keep-alive to reuse TLS connections during multi-request operations like `estimate-all` and `watch`.

## Background
Commands like `estimate-all` and `watch` execute numerous sequential or concurrent RPC requests. Establishing a new TLS handshake for every call adds 100-300ms of latency per call. Connection pooling reuses open sockets, reducing total execution time significantly.

## Acceptance criteria
- [ ] `RpcClient` reuses a shared `reqwest::Client` instance across all requests within a command run
- [ ] Configures connection pool idle timeout (90s) and TCP keep-alive (30s)
- [ ] Benchmarks show reduced wall-clock time for `estimate-all` across 10+ functions
- [ ] Closes connections cleanly upon command exit

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-connection-pooling`

Key files:
- `src/rpc/client.rs`
- `src/rpc/mod.rs`

In `src/rpc/client.rs`, construct `reqwest::Client::builder().tcp_keepalive(Duration::from_secs(30)).pool_idle_timeout(Duration::from_secs(90)).build()?` and reuse the client instance in `RpcClient`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom HTTP/3 QUIC transport implementation
- Multi-host round-robin load balancing
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add `--rpc-url` validation before simulation" "enhancement, network, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Perform a lightweight URL schema and reachability check on custom `--rpc-url` arguments before starting heavy WASM parsing and simulation.

## Background
When users provide malformed URLs (e.g. missing `https://` scheme or typo in domain), the error currently bubbles up from deep inside JSON deserialization or XDR decoding with a confusing error message. Validating the URL early produces clear actionable feedback.

## Acceptance criteria
- [ ] Validates that `--rpc-url` is a valid HTTP or HTTPS URI
- [ ] Rejects non-HTTP schemes (e.g. `ftp://`, `file://`) with helpful error message
- [ ] Provides immediate error if port number is invalid or host is missing
- [ ] Unit tests cover valid and invalid URL formats

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-url-validation`

Key files:
- `src/rpc/client.rs`
- `src/error.rs`
- `src/main.rs`

Use `reqwest::Url::parse(url)` to validate URL structure. Add `AppError::InvalidRpcUrl(String)` variant to `src/error.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated DNS lookups on offline networks
- SSL certificate pinning
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add retry with configurable max attempts (`--max-retries`)" "enhancement, network, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Expose `--max-retries <count>` CLI argument to configure the maximum number of retry attempts for RPC requests.

## Background
Different environments require different retry tolerances: CI environments may want aggressive retries (5+ attempts) to avoid flaky runs, while interactive terminal users may prefer failing fast (0 retries).

## Acceptance criteria
- [ ] `--max-retries <N>` global CLI flag added (default: 3)
- [ ] Setting `--max-retries 0` disables retries completely
- [ ] Passed to the RPC retry loop in `src/rpc/client.rs`
- [ ] Included in CLI `--help` text with clear description

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-max-retries-flag`

Key files:
- `src/cli.rs`
- `src/rpc/client.rs`
- `src/main.rs`

Add `#[arg(long, global = true, default_value = "3")] max_retries: u32` in `src/cli.rs`. Pass to `RpcClient` constructor.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Per-error-type custom retry policies
- Interactive prompt to retry on failure
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add request/response payload logging in debug mode" "enhancement, network, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Log full JSON-RPC request and response bodies when `RUST_LOG=debug` or `--verbose` is enabled.

## Background
When debugging why a simulation failed with `TransactionSimulationResultFailed`, inspecting the exact JSON-RPC payload sent to Stellar Core and the exact raw error string returned is critical for troubleshooting.

## Acceptance criteria
- [ ] Logs outgoing JSON-RPC method name, ID, and parameters to stderr
- [ ] Logs incoming JSON-RPC HTTP status, response headers, and response body to stderr
- [ ] Masks sensitive headers if authorization tokens are passed (see #60)
- [ ] Does not alter stdout or disrupt `--json` parsing

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-debug-logging`

Key files:
- `src/rpc/client.rs`
- `src/main.rs`

In `src/rpc/client.rs`, before sending `reqwest::Request`, format JSON with `serde_json::to_string(&body)` and emit with `eprintln!("[RPC REQ] {}", ...)` if verbose is active.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Packet capturing at TCP level
- Writing payloads to disk files
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add `rpc health` command to check network endpoint status" "enhancement, network, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add an `rpc health` subcommand that pings the target RPC endpoint via `getHealth` and `getNetwork`, printing status, protocol version, and latency.

## Background
Before running complex batch simulations, users want a quick sanity check to ensure their selected RPC endpoint is healthy, synchronized, and running the expected protocol version.

## Acceptance criteria
- [ ] `soroban-cost-estimator rpc health [--network <name>] [--rpc-url <url>]` command implemented
- [ ] Calls `getHealth` and `getNetwork` RPC methods
- [ ] Displays: Health Status ('healthy'), Network Passphrase, Protocol Version, Core Version, Round-trip latency (ms)
- [ ] `--json` emits structured health report
- [ ] Exits with code 0 if healthy, code 1 if degraded or unreachable

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-health-cmd`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/client.rs`

Add `Rpc` subcommand with `Health` variant in `src/cli.rs`. Implement `get_health()` and `get_network()` methods in `src/rpc/client.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Continuous RPC health monitoring daemon (see #52)
- Alerting integrations
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add support for custom HTTP headers (`--header`)" "enhancement, network, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--header <KEY=VALUE>` flag to pass custom HTTP headers (such as API keys, Bearer tokens, or rate-limit bypass headers) to RPC endpoints.

## Background
Commercial Soroban RPC providers (e.g. QuickNode, Blockdaemon, NowNodes) require API keys passed via `Authorization: Bearer <token>` or `x-api-key: <key>` headers. Supporting custom headers allows developers to use authenticated private endpoints.

## Acceptance criteria
- [ ] `--header <KEY=VALUE>` / `-H <KEY=VALUE>` flag accepted multiple times across all commands
- [ ] Headers are attached to all outgoing RPC HTTP requests
- [ ] Rejects malformed header arguments (missing `=`, invalid header characters) with clear error
- [ ] Sensitive headers (`Authorization`, `x-api-key`, `api-key`) are redacted in verbose logs

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-custom-headers`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/client.rs`

Add `#[arg(long = "header", short = '''H''', global = true)] headers: Vec<String>` in `src/cli.rs`. Parse into `reqwest::header::HeaderMap` and apply to `reqwest::ClientBuilder` in `src/rpc/client.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- OAuth2 token refresh flow
- Keyring OS credential storage
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add client-side rate limiting to prevent RPC 429 throttling" "enhancement, network, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Implement client-side token bucket rate limiting with a `--rate-limit <rps>` option to govern request frequency during batch simulations.

## Background
When `estimate-all` runs parallel simulations across dozens of functions, bursting 50+ requests simultaneously can trigger server-side IP rate limits (HTTP 429). Client-side rate limiting smooths request spikes.

## Acceptance criteria
- [ ] `--rate-limit <rps>` flag added to `estimate-all` and `watch` (requests per second, default: unlimited or 10 rps)
- [ ] Limits concurrent/consecutive outbound HTTP requests using async governor or token bucket
- [ ] Prevents burst 429 errors when running `estimate-all` on contracts with large function specs
- [ ] Unit test asserts rate limiter paces execution intervals accurately

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-rate-limiting`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/rpc/client.rs`

Use `tokio::time::sleep` or a simple leaky bucket token counter in `RpcClient` to ensure minimum spacing between outgoing requests.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Distributed rate limiter synchronization
- Dynamic rate limiting based on response headers
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add in-memory caching for repeated config setting fetches" "enhancement, performance, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Cache `getLedgerEntries` network config setting results in memory during a single CLI command run to eliminate redundant network roundtrips.

## Background
During `estimate-all`, the tool needs network config settings (`ConfigSettingContractComputeV0`, `ConfigSettingContractLedgerCostV0`, etc.) to calculate fees for every function. Fetching config settings over network for each function is wasteful and slow.

## Acceptance criteria
- [ ] Fetches network config entries once per command execution and shares them across all function fee evaluations
- [ ] Eliminates duplicate `getLedgerEntries` RPC calls during `estimate-all`
- [ ] Reduces total network roundtrips from `2 * N` to `N + 1` for N functions
- [ ] Tests verify that config settings are fetched only once

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-config-cache`

Key files:
- `src/rpc/config.rs`
- `src/main.rs`

In `cmd_estimate_all` in `src/main.rs`, fetch `NetworkConfig` once up front and pass `&NetworkConfig` into each function evaluation loop.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Inter-process shared memory caching
- Persistent disk caching of network config across runs without snapshot
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add `--connect-timeout` separate from total request timeout" "enhancement, network, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--connect-timeout <seconds>` to distinguish between TCP connection establishment timeout and response read timeout.

## Background
A host that is completely unreachable (dead IP, dropped SYN packets) should fail connection within 3-5 seconds, whereas a complex simulation on a live node might need 30 seconds to compute. Separating connect timeout from request timeout prevents long hangs on dead endpoints.

## Acceptance criteria
- [ ] `--connect-timeout <secs>` CLI flag added (default: 5 seconds)
- [ ] Configures `reqwest::ClientBuilder::connect_timeout`
- [ ] Produces distinct error on connect failure: 'Failed to establish connection to RPC host within X seconds'
- [ ] Unit test asserts connect timeout configuration

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-connect-timeout`

Key files:
- `src/cli.rs`
- `src/rpc/client.rs`
- `src/error.rs`

Add `connect_timeout: Option<u64>` to `Cli`. Pass `Duration::from_secs(connect_timeout)` to `reqwest::ClientBuilder::connect_timeout`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- TCP SYN retry tweaking at kernel level
- SOCKS5 proxy configuration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add network latency tracking and reporting" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Measure and display RPC network round-trip time in milliseconds in standard cost reports and JSON output.

## Background
Developers and CI logs benefit from knowing how much time was spent waiting on RPC network simulation vs local computation and decoding.

## Acceptance criteria
- [ ] Records simulation round-trip duration in milliseconds (`Instant::now()` around RPC call)
- [ ] Displays 'Simulation latency: X ms' in table output footer
- [ ] Includes `simulation_duration_ms: u64` in `--json` output
- [ ] Tests assert that `simulation_duration_ms > 0` on live/mock simulation responses

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-latency-tracking`

Key files:
- `src/rpc/simulate.rs`
- `src/report/cost_report.rs`
- `src/main.rs`

Capture `start.elapsed()` in `simulate_transaction` and store `duration_ms` in `CostReport`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Distributed OpenTelemetry tracing spans
- Network packet jitter analysis
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add support for WebSocket RPC endpoint subscriptions" "enhancement, network, complexity: high, Stellar Wave" "$(cat <<'EOF'
## Summary
Support connecting to Soroban RPC over WebSockets (`wss://`) for event streaming and real-time ledger close notifications in `watch` mode.

## Background
In `watch` mode, polling over HTTP every few seconds adds unnecessary latency and network traffic. Supporting WebSocket subscriptions allows the tool to receive instant ledger close notifications directly from the RPC node.

## Acceptance criteria
- [ ] `watch` command supports `wss://` RPC URLs
- [ ] Subscribes to ledger close events over WebSocket connection
- [ ] Re-checks network config immediately when a new ledger closes instead of fixed interval polling
- [ ] Automatically reconnects with exponential backoff on WebSocket disconnection
- [ ] Falls back gracefully to HTTP polling if WebSocket is unsupported or fails

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-websocket-support`

Key files:
- `Cargo.toml`
- `src/rpc/mod.rs`
- `src/main.rs`

Add `tokio-tungstenite` dependency. Implement WebSocket message listener for ledger close events in `src/rpc/ws.rs` and trigger snapshot diff on event.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Full bi-directional RPC multiplexing
- Custom binary protocol decoding
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add failover to secondary fallback RPC endpoint (`--rpc-fallback-url`)" "enhancement, network, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--rpc-fallback-url <url>` to automatically fail over to a backup RPC endpoint if the primary endpoint is down or returns repeated 5xx errors.

## Background
Public endpoints can experience temporary downtime or network partitions. Providing a secondary fallback endpoint ensures continuous operation for critical CI pipelines and automated monitoring.

## Acceptance criteria
- [ ] `--rpc-fallback-url <url>` global CLI argument added
- [ ] If primary endpoint fails after retries (or returns 502/503/504), switches to fallback URL and retries request
- [ ] Logs notice to stderr: 'Primary RPC failed, failing over to fallback endpoint: <url>'
- [ ] Integration test verifies automatic fallback when primary URL points to an invalid host

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-fallback-url`

Key files:
- `src/cli.rs`
- `src/rpc/client.rs`
- `src/main.rs`

Add `rpc_fallback_url: Option<String>` in `src/cli.rs`. In `RpcClient`, maintain primary and fallback clients, switching client on fatal primary errors.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Round-robin load balancing across multiple active endpoints
- Endpoint health ranking algorithm
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(rpc): add request deduplication for batch function simulations" "enhancement, performance, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Deduplicate identical RPC requests when simulating functions with identical arguments or during multi-function contract evaluations.

## Background
When inspecting multiple functions in a batch, shared queries (like WASM bytecode upload simulation or fee rate lookup) can be deduplicated to reduce RPC load and speed up test execution.

## Acceptance criteria
- [ ] Identical in-flight simulation requests share a single underlying future (join-handle / broadcast)
- [ ] Prevents duplicate concurrent HTTP requests for identical payloads
- [ ] Reduces total HTTP requests in concurrent batch execution modes
- [ ] Unit tests verify deduplication under concurrent calls

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/rpc-request-dedup`

Key files:
- `src/rpc/client.rs`
- `src/rpc/simulate.rs`

Use an in-flight request map (`Arc<Mutex<HashMap<RequestHash, SharedFuture>>>`) in `RpcClient` to coalesce concurrent identical requests.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Persistent cross-process request queue
- Distributed deduplication
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# WASM ANALYSIS & INSPECTION (#68–77)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "feat(wasm): validate `--arg` values against contract spec types" "enhancement, wasm, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Validate user-provided `--arg key=value` CLI parameters against the decoded `contractspecv0` function signature before building transaction XDR.

## Background
Currently `--arg` values use heuristic type inference (parsing numbers as i64/u64, bools as boolean, and falling back to strings). If a contract function expects a `symbol` or `address` but receives an integer or string, the simulation fails deep in Stellar Core RPC with obscure XDR decode errors. Validating against the WASM contract spec catches type mismatches upfront with clear, actionable error messages.

## Acceptance criteria
- [ ] Extracts parameter type definitions from `contractspecv0` for the target function in `src/wasm/parser.rs`
- [ ] Validates each `--arg <name>=<val>` against expected spec type (`bool`, `i32`, `i64`, `u32`, `u64`, `i128`, `u128`, `symbol`, `string`, `address`)
- [ ] Coerces ambiguous values to the expected type (e.g. string representation of symbol into `ScVal::Symbol`)
- [ ] Provides clear error naming the parameter, expected type, and actual provided value on mismatch
- [ ] Tests verify validation and coercion for all supported Soroban primitive types

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-spec-arg-validation`

Key files:
- `src/wasm/parser.rs`
- `src/xdr_helper.rs`
- `src/main.rs`

In `src/wasm/parser.rs`, return `ScSpecFunctionV0` params. In `src/xdr_helper.rs`, implement `parse_arg_with_spec(key, val, &ScSpecTypeDef) -> Result<ScVal, AppError>`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Complex recursive user-defined structs from CLI args (handled via JSON input)
- Interactive argument editor (see #46)
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): detect and display WASM entry points and memory structure" "enhancement, wasm, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Inspect WASM bytecode sections to display declared exports, start function, imported host functions, and initial/maximum linear memory limits.

## Background
Soroban contracts compile to WebAssembly with specific constraints on memory allocation and imported host functions (`env._` imports). Providing structural analysis of the WASM file helps developers understand why memory fees or initialization costs are high.

## Acceptance criteria
- [ ] Parses memory section to extract initial memory pages and maximum memory pages
- [ ] Enumerates imported host functions from `env` module (e.g. host storage, crypto, context functions)
- [ ] Identifies contract export functions and table entries
- [ ] Displays memory configuration in `--verbose` or `--wasm-info` modes
- [ ] Warns if initial memory pages exceed standard Soroban limits (e.g. > 16 pages)

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-memory-inspection`

Key files:
- `src/wasm/parser.rs`
- `src/wasm/mod.rs`
- `src/report/cost_report.rs`

Use `wasmparser::Parser` to traverse `Payload::MemorySection` and `Payload::ImportSection`. Store memory limits and imported function count in a new `WasmStructureSummary` struct.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Full WASM decompiler/disassembler
- WASM control-flow graph visualization
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): compute and display SHA-256 hash before simulation" "enhancement, wasm, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Compute the SHA-256 hash of the input WASM binary immediately upon loading and display it in report headers and logs.

## Background
In Soroban, contract WASM binaries are identified on-chain by their 32-byte SHA-256 hash (the executable hash). Displaying this hash clearly allows developers to verify that the local `.wasm` matches their deployed on-chain contract code.

## Acceptance criteria
- [ ] Computes SHA-256 hash of WASM bytes using `sha2::Sha256`
- [ ] Formats hash as hex string (e.g. `ea14bca9...`) and includes in `CostReport`
- [ ] Displays hash prominently in table header and JSON output (`wasm_hash`)
- [ ] Unit tests verify hash calculation against known fixture WASM files

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-sha256-display`

Key files:
- `src/wasm/parser.rs`
- `src/report/cost_report.rs`
- `src/main.rs`

In `src/wasm/parser.rs`, compute `hex::encode(sha2::Sha256::digest(bytes))` during `parse_wasm()` and include in `WasmMetadata`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Uploading WASM hash to IPFS
- Generating contract address from hash offline without deployer key
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): support `contractmeta` custom section parsing" "enhancement, wasm, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Extract and display contract metadata (name, version, description, author, SDK version) embedded in the Soroban `contractmeta` custom section.

## Background
Soroban SDK contracts embed metadata key-value pairs in the `contractmeta` WASM custom section. Reading this metadata provides rich contextual information in cost reports and CLI listings.

## Acceptance criteria
- [ ] Locates `contractmeta` custom section using `wasmparser`
- [ ] Decodes `ScMetaV0` / `ScMetaEntry` XDR structures
- [ ] Extracts key-value pairs (e.g. `rs_sdk_version`, `name`, `version`)
- [ ] Includes extracted metadata in table output and `--json` payload
- [ ] Handles WASM binaries without `contractmeta` section gracefully without error

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-contract-meta`

Key files:
- `src/wasm/parser.rs`
- `src/wasm/mod.rs`
- `src/report/cost_report.rs`

In `src/wasm/parser.rs`, match `wasmparser::Payload::CustomSection` where `name == "contractmeta"`. Decode with `stellar_xdr::curr::ScMetaEntry::from_xdr`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Writing/modifying metadata in WASM binary
- Verifying author cryptographic signatures
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): add WASM section size breakdown summary" "enhancement, wasm, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Analyze and display the byte size of each WASM section (Code, Data, Custom `contractspecv0`, Type, Export, Import) in cost reports.

## Background
WASM size directly impacts the upload fee and rent calculation on Soroban. A detailed section size breakdown helps contract authors identify why their contract is large (e.g. bloated `contractspecv0`, oversized data segments, or debug info).

## Acceptance criteria
- [ ] Iterates WASM sections and records byte size of: Code (bytecode), Data (constants), Custom (`contractspecv0`, `contractmeta`, etc.), Type/Function declarations, and other sections
- [ ] Calculates percentage of total file size for each section
- [ ] Displays section breakdown table in `--verbose` or `--wasm-info` mode
- [ ] Includes `section_sizes` map in `--json` output
- [ ] Test verifies section sizes sum to total WASM byte length

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-section-sizes`

Key files:
- `src/wasm/parser.rs`
- `src/report/cost_report.rs`

In `src/wasm/parser.rs`, record byte offsets of each section header and content to compute exact size per section.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated WASM optimization/stripping (`wasm-opt`)
- Function-level bytecode size profiling
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): add `--wasm-info` command to display contract metadata without RPC" "enhancement, wasm, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `wasm info <file.wasm>` subcommand that parses and displays all local contract metadata (functions, params, types, sections, metadata) completely offline.

## Background
Developers often want to inspect contract function signatures, exported methods, and metadata without running a network simulation or needing an internet connection.

## Acceptance criteria
- [ ] `soroban-cost-estimator wasm info <file.wasm>` command implemented
- [ ] Displays: WASM Size, SHA-256 Hash, Exported functions with argument types and return types, Embedded metadata (SDK version, contract name), Section size summary
- [ ] `--json` outputs full parsed AST/spec metadata
- [ ] Requires zero network access
- [ ] Clear error message if the file is not a valid WebAssembly binary

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-info-command`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/wasm/parser.rs`

Add `Wasm` subcommand with `Info { wasm: PathBuf, #[arg(long)] json: bool }` in `src/cli.rs`. Implement `cmd_wasm_info` calling `parse_wasm`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Decompiling function bodies into Rust source code
- Interactive function debugger
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): add WASM memory limit validation against Soroban protocol constraints" "enhancement, wasm, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Validate WASM memory and table constraints against network limits before simulation, warning if limits are exceeded.

## Background
Soroban enforces hard limits on maximum WASM linear memory pages (typically 128 MB or 2048 pages) and maximum WASM file sizes. Validating these constraints locally prevents confusing simulation failures from the network.

## Acceptance criteria
- [ ] Checks WASM file size against `ConfigSettingContractComputeV0::tx_max_contract_size` (or default 64KB/128KB limits)
- [ ] Checks declared initial/max memory pages against Soroban limits
- [ ] Emits warning or returns error if WASM exceeds maximum deployable size
- [ ] Unit tests cover WASM binaries within and exceeding size limits

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-memory-validation`

Key files:
- `src/wasm/parser.rs`
- `src/error.rs`

Add validation method `validate_wasm_limits(&self, max_size: u32, max_pages: u32) -> Result<(), AppError>` in `src/wasm/parser.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Dynamic heap memory profiling during execution
- Modifying WASM memory headers
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): add batch support for evaluating multiple WASM files" "enhancement, wasm, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Allow passing multiple `--wasm <file>` or a directory of `.wasm` files to `estimate` and `estimate-all` to generate a multi-contract cost summary.

## Background
Complex dApps (e.g. DEXs, lending protocols) consist of multiple cooperating contracts (e.g. factory, pool, token, router). Estimating costs across all workspace contracts in a single command provides a complete protocol cost overview.

## Acceptance criteria
- [ ] Accepts multiple `--wasm <path>` flags or `--wasm-dir <dir>`
- [ ] Parses and simulates each WASM binary sequentially or concurrently
- [ ] Generates an aggregated summary table displaying each contract's upload cost, function count, and min/max invocation fees
- [ ] Supports `--json` structured output containing all contracts
- [ ] Gracefully continues if one contract fails, reporting errors in the summary

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-batch-support`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/report/cost_report.rs`

Update `wasm: Vec<PathBuf>` in CLI definitions. In `src/main.rs`, iterate over files and build `BatchCostReport`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Cross-contract inter-contract call simulation graph
- Automated contract deployment
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): detect WASM compression and recommend `soroban contract optimize`" "enhancement, wasm, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Detect unoptimized WASM binaries (e.g. debug symbols present, uncompressed) and output optimization recommendations.

## Background
Unoptimized WASM builds contain large `name` custom sections and unstripped debug symbols, leading to inflated deployment costs. Detecting these issues locally and recommending `soroban contract optimize` saves users fees.

## Acceptance criteria
- [ ] Detects presence of `name` or `.debug_*` sections in the WASM file
- [ ] Calculates potential size reduction estimate (typically 40-70%)
- [ ] Prints tip to stderr: '💡 Tip: Unoptimized WASM detected (contains debug symbols). Run `soroban contract optimize` or `wasm-opt` to reduce upload cost by ~X%'
- [ ] Tip is omitted when `--quiet` or `--json` is active

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-optimization-detection`

Key files:
- `src/wasm/parser.rs`
- `src/main.rs`

In `src/wasm/parser.rs`, check for `custom_section.name() == "name" || custom_section.name().starts_with(".debug")`. Set `has_debug_symbols: bool` in `WasmMetadata`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Bundling `wasm-opt` binary directly in this crate
- Automatically rewriting the file on disk
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(wasm): extract and display contract spec docs from `contractspecv0`" "enhancement, wasm, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Parse docstrings embedded in `ScSpecFunctionV0` and display function documentation in CLI help and reports.

## Background
When Rust smart contracts are compiled with doc comments (`/// ...`), the Soroban SDK embeds these descriptions in `contractspecv0`. Surfacing these docs in `estimate-all` and `wasm info` gives developers instant context on what each function does.

## Acceptance criteria
- [ ] Extracts `doc` field from `ScSpecFunctionV0` structs during spec decoding
- [ ] Displays docstrings in `wasm info` and `estimate-all` table outputs
- [ ] Includes `doc` field in `--json` function metadata
- [ ] Handles multi-line doc comments cleanly

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/wasm-spec-docs`

Key files:
- `src/wasm/parser.rs`
- `src/report/cost_report.rs`

Update `FunctionMetadata` in `src/wasm/parser.rs` to include `doc: Option<String>`. Read from `ScSpecFunctionV0::doc`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Generating static HTML documentation site
- Markdown rendering inside terminal cells
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# REPORTING, VISUALIZATIONS & FORMATS (#78–92)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "feat(report): human-readable config setting names in all outputs" "enhancement, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Replace raw numeric `ConfigSettingId` enum numbers (e.g. `0`, `1`, `4`) with human-readable names like 'Contract Compute V0', 'Contract Ledger Cost V0', 'State Archival' across all outputs.

## Background
When printing config snapshot diffs or setting lookups, raw numeric IDs require developers to consult Stellar XDR definitions to understand which setting was changed. Human-readable names provide immediate clarity.

## Acceptance criteria
- [ ] Maps all `ConfigSettingId` variants to friendly descriptive names (e.g. `ConfigSettingContractComputeV0` -> 'Contract Compute Settings', `ConfigSettingContractLedgerCostV0` -> 'Contract Ledger Cost Settings')
- [ ] Displays friendly names in `config diff` tables and diff headers
- [ ] Includes friendly names alongside raw IDs in `--json` output
- [ ] Unit test asserts complete mapping coverage for all current Stellar Protocol config settings

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-human-config-names`

Key files:
- `src/config_snapshot/model.rs`
- `src/config_snapshot/diff.rs`
- `src/report/cost_report.rs`

Implement a helper `pub fn config_setting_human_name(id: &ConfigSettingId) -> &'static str` in `src/config_snapshot/model.rs` and use in diff formatting.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Dynamic localized translations
- Custom setting rename aliases
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add fee breakdown percentage columns to cost reports" "enhancement, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Display the relative percentage contribution of each fee component (CPU Instructions, Storage Read/Write, Transaction Size, Base Fee, Rent) to the total fee.

## Background
Understanding where money is being spent (e.g. 85% on Storage Write vs 10% on CPU) helps contract developers immediately identify the highest-ROI area for cost optimization.

## Acceptance criteria
- [ ] Calculates percentage of total fee for each component: CPU instructions fee, Storage read/write fee, Transaction size fee, Base fee, Rent fee
- [ ] Displays percentages in table output (e.g. `1,250 stroops (75.4%)`)
- [ ] Includes `fee_percentages` map in `--json` output
- [ ] Percentages sum to 100.0% (handling rounding gracefully)

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-fee-percentages`

Key files:
- `src/report/cost_report.rs`
- `src/report/fee_calc.rs`

Compute `(component_fee as f64 / total_fee as f64) * 100.0` in `src/report/cost_report.rs` and format in table cells.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Interactive graphical pie chart in browser
- Cost optimization automated rewrites
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add aggregated cost summary line at the end of `estimate-all`" "enhancement, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Print an aggregated summary row at the bottom of `estimate-all` showing total functions evaluated, total CPU consumed, average fee, and total protocol fee.

## Background
When evaluating contracts with 15+ functions, users have to manually sum up numbers or calculate averages. An aggregated summary footer provides immediate high-level metrics.

## Acceptance criteria
- [ ] Prints summary footer row in `estimate-all` table with: Total functions count, Min fee, Max fee, Average fee, Total CPU instruction range
- [ ] Summary row is visually separated with table border styling
- [ ] Includes `summary` object in `estimate-all --json` output
- [ ] Snapshot tests verify formatting with single and multi-function contracts

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-estimate-all-summary`

Key files:
- `src/main.rs`
- `src/report/cost_report.rs`

In `src/report/cost_report.rs`, implement `format_estimate_all_table(&[CostReport])` including a footer row using `comfy_table::Row`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Statistical regression analysis
- Comparing summaries across multiple contracts
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add historical cost trend comparison in cost reports" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Show historical fee progression across multiple previous cached runs for the same contract function.

## Background
Tracking how a function's cost evolved over the last 3-5 builds allows developers to catch fee regressions early in development.

## Acceptance criteria
- [ ] When `--history` flag is passed, queries up to 5 previous cached estimates for the same contract function
- [ ] Renders a mini trend table showing: Timestamp/Ledger, CPU Instructions, Total Fee, and Delta vs current run
- [ ] Highlights regressions (cost increases) in red and improvements (cost reductions) in green
- [ ] Includes `history` array in `--json` output when requested

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-cost-history`

Key files:
- `src/cli.rs`
- `src/cache.rs`
- `src/report/cost_report.rs`
- `src/main.rs`

Add `history: bool` in `src/cli.rs`. In `src/cache.rs`, implement `load_estimate_history(wasm_hash, fn_name, limit)`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- External telemetry server upload
- Long-term database storage engine
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add resource limit warnings when nearing maximum network constraints" "enhancement, ux, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Warn developers when a function's resource consumption approaches or exceeds 80% of network limits (e.g. CPU instructions, footprint read/write bytes, tx size).

## Background
Soroban transactions fail if they exceed protocol resource limits (`tx_max_instructions`, `tx_max_read_bytes`, `tx_max_write_bytes`, `tx_max_size`). Proactively warning when usage nears limits prevents production failures under dynamic state growth.

## Acceptance criteria
- [ ] Compares simulated resources against `ConfigSettingContractComputeV0` and `ConfigSettingContractLedgerCostV0` limits
- [ ] Emits warning banner if CPU instructions >= 80% of `tx_max_instructions`
- [ ] Emits warning banner if storage read/write bytes or entries >= 80% of max limits
- [ ] Emits warning banner if transaction size >= 80% of `tx_max_size`
- [ ] Includes `warnings` array in JSON output
- [ ] Tests cover warning triggering at 80% threshold

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-resource-warnings`

Key files:
- `src/report/cost_report.rs`
- `src/report/fee_calc.rs`

Create `pub fn check_resource_limits(report: &CostReport, config: &NetworkConfig) -> Vec<ResourceWarning>` in `src/report/cost_report.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated code refactoring suggestions to reduce CPU
- Simulating dynamic network congestion
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add actionable cost optimization suggestions" "enhancement, ux, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Analyze simulation resource footprints and generate contextual, actionable optimization tips to reduce gas fees.

## Background
New Soroban developers often don't realize that storage writes and ledger footprint size cost significantly more than CPU instructions. Providing contextual advice based on the simulation footprint accelerates cost reduction.

## Acceptance criteria
- [ ] Analyzes simulation breakdown to identify dominant cost factors (e.g. High storage writes, large WASM upload size, large argument payloads)
- [ ] Generates targeted suggestions (e.g. 'Tip: Writing 3 ledger entries accounts for 72% of total fee. Consider combining related state into a single entry.')
- [ ] Generates WASM size optimization suggestion if WASM > 30KB
- [ ] Displays suggestions in terminal output and includes in JSON report under `suggestions`
- [ ] Omitted when `--quiet` is passed

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-cost-suggestions`

Key files:
- `src/report/cost_report.rs`
- `src/main.rs`

Implement `pub fn generate_optimization_tips(report: &CostReport) -> Vec<String>` analyzing `FeeBreakdown` proportions.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated AST refactoring
- AI code generator integrations
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add CSV export format for `estimate-all`" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Support exporting `estimate-all` results as RFC 4180 compliant CSV for spreadsheet analysis.

## Background
Financial analysts and developers building cost models need to import multi-function contract estimates into Excel, Google Sheets, or data pipelines.

## Acceptance criteria
- [ ] `--format csv` flag outputs valid CSV with headers: `Function,Status,CPU_Instructions,RAM_Bytes,Read_Entries,Write_Entries,Read_Bytes,Write_Bytes,Total_Fee_Stroops,Total_Fee_XLM`
- [ ] Properly escapes function names or error strings containing commas or quotes
- [ ] Outputs clean CSV to stdout or `--output` file
- [ ] Tests verify CSV output against standard RFC 4180 parsing

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-csv-format`

Key files:
- `src/report/cost_report.rs`
- `src/main.rs`

Implement `format_estimate_all_csv(&[CostReport]) -> String` writing comma-separated lines.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Direct Excel .xlsx binary encoding
- Direct Google Sheets API integration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add Markdown table export for GitHub PR comments" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Support exporting cost estimates as GitHub-flavored markdown tables suitable for automated CI PR comments.

## Background
Teams running cost estimation in GitHub Actions want to post the cost breakdown as a comment on pull requests. Markdown table formatting renders natively on GitHub without terminal escape codes.

## Acceptance criteria
- [ ] `--format markdown` outputs clean GitHub-flavored markdown table (`| Function | CPU | Fee (XLM) |`)
- [ ] Includes summary stats and collapsible details sections for complex breakdowns
- [ ] Compatible with GitHub Actions PR commenter bots
- [ ] Tests assert valid Markdown table formatting

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-markdown-format`

Key files:
- `src/report/cost_report.rs`
- `src/main.rs`

Implement `format_cost_report_markdown(&CostReport) -> String` using Markdown table syntax.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated GitHub API posting (CI action uses output file)
- HTML markdown styling
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add ANSI color coding for pricing changes in `config diff`" "enhancement, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Color-code diff output in terminal: price increases in bold red, price decreases in bold green, and non-pricing changes in cyan.

## Background
When scanning a 20-row config diff table, visual color coding allows developers to immediately spot fee-rate hikes versus fee-rate reductions at a glance.

## Acceptance criteria
- [ ] Positive percentage changes (fee increases) render in Red (`[31m`)
- [ ] Negative percentage changes (fee decreases) render in Green (`[32m`)
- [ ] Non-pricing configuration changes render in Dim/Cyan (`[36m`)
- [ ] Respects `--color=never` or `NO_COLOR` environment variable by stripping escape codes
- [ ] Tests verify color coding logic

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-ansi-diff-colors`

Key files:
- `src/config_snapshot/diff.rs`
- `src/report/cost_report.rs`

Use comfy-table cell styling or ANSI color helper to apply colors conditionally based on `is_pricing_change` and delta sign.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom user-defined theme RGB files
- TrueColor 24-bit gradients
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add cost projection for batch invocations (e.g. 100x, 10k calls)" "enhancement, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--project <N,N,N>` flag to `estimate` to display projected operational costs for 100, 1,000, 10,000, or 100,000 invocations.

## Background
Protocol designers budgeting monthly operational costs (e.g. oracle updates, keeper bots) need to know total projected spend over thousands of daily transactions.

## Acceptance criteria
- [ ] `--project <counts>` flag added (e.g. `--project 100,1000,10000`, default: `100,1000,10000` when enabled)
- [ ] Displays projection table: Invocations count, Total Stroops, Total XLM, and USD estimate if XLM price is known (or pure XLM)
- [ ] Included in `--json` output under `projections` array
- [ ] Unit tests verify multiplication math without 64-bit integer overflow

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-cost-projections`

Key files:
- `src/cli.rs`
- `src/report/cost_report.rs`
- `src/main.rs`

Add `project: Option<Vec<u64>>` in `src/cli.rs`. Calculate `total_fee * count` using checked multiplication.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Live cryptocurrency price API fetching
- Gas price volatility forecasting
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add min/max/avg fee range statistics in `estimate-all`" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Calculate and report statistical distribution metrics (min, max, mean, median, standard deviation) of fees and CPU usage across all contract functions in `estimate-all`.

## Background
Contracts often have cheap 'getter' functions (5,000 stroops) alongside expensive 'batch processing' functions (500,000 stroops). Range and variance statistics give a quick summary of contract cost profile.

## Acceptance criteria
- [ ] Computes minimum, maximum, mean, and median fee across all simulated functions
- [ ] Computes minimum, maximum, and mean CPU instruction counts
- [ ] Displays distribution box in `estimate-all` summary
- [ ] Includes `fee_distribution` object in JSON output
- [ ] Unit tests test statistical calculations on edge cases (1 function, identical fees)

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-fee-statistics`

Key files:
- `src/report/cost_report.rs`
- `src/main.rs`

Implement `calculate_distribution_stats(reports: &[CostReport]) -> FeeDistribution` in `src/report/cost_report.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Histograms in terminal
- Outlier machine-learning detection
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add simulation ledger sequence and age in reports" "enhancement, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Display the latest network ledger sequence number and snapshot timestamp in the report header.

## Background
Fee estimates and contract state are tied to a specific ledger sequence. Displaying the ledger sequence and age ensures users know how fresh the simulation environment is.

## Acceptance criteria
- [ ] Extracts latest ledger sequence number from simulation response or `getLatestLedger`
- [ ] Displays 'Simulated at ledger sequence: 1,234,567' in report header
- [ ] Includes `ledger_sequence: u32` in JSON report schema
- [ ] Tests verify ledger sequence propagation to output

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-ledger-sequence`

Key files:
- `src/rpc/simulate.rs`
- `src/report/cost_report.rs`

Extract `latest_ledger` from `SimulateTransactionResponse` and pass to `CostReport::new`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Simulating against historical past ledgers (requires archival RPC node)
- Tracking ledger finality confirmations
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add configurable output precision for XLM fee values (`--precision`)" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--precision <N>` flag to control the number of decimal places displayed for XLM fee values (default: 7 decimal places).

## Background
Stellar amounts are denominated in stroops (1 XLM = 10,000,000 stroops, 7 decimals). For display, some users prefer standard currency precision (e.g. 4 decimals) while others require full 7-decimal fidelity.

## Acceptance criteria
- [ ] `--precision <N>` flag added (range 0 to 7, default: 7)
- [ ] Controls decimal places formatted in table outputs and text summaries
- [ ] Stroop integer calculations remain exact and unmodified
- [ ] Unit tests test formatting with precision 2, 4, 7

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-precision-flag`

Key files:
- `src/cli.rs`
- `src/report/cost_report.rs`
- `src/main.rs`

Add `#[arg(long, global = true, default_value = "7")] precision: usize` in `src/cli.rs`. Use in `stroops_to_xlm_formatted` helper.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom thousand separators (, vs .)
- Scientific exponential notation
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add ASCII horizontal bar chart for fee breakdown in terminal" "enhancement, ux, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Render a clean ASCII/Unicode horizontal bar chart visualizing fee distribution (CPU, Storage, Bandwidth, Rent) in terminal output.

## Background
A visual bar chart (`[████████░░░░] CPU: 65% | [████░░░░░░░░] IO: 35%`) provides instant intuitive understanding of cost allocation in terminal reports.

## Acceptance criteria
- [ ] Renders ASCII/Unicode horizontal bar chart when terminal width permits (>= 80 columns)
- [ ] Visualizes proportions for CPU, Read/Write Storage, Bandwidth, and Rent
- [ ] Automatically scales bar width to terminal column width
- [ ] Disabled in `--quiet` or non-TTY piped output
- [ ] Unit test asserts chart string generation with mock breakdowns

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-ascii-chart`

Key files:
- `src/report/cost_report.rs`
- `src/report/mod.rs`

Implement `render_fee_bar_chart(breakdown: &FeeBreakdown, width: usize) -> String` using block characters (`█`, `▓`, `▒`, `░`).

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- SVG/PNG image generation (see #39)
- Interactive mouse-hover charts in terminal
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(report): add side-by-side cost report diff formatting" "enhancement, ux, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Implement a side-by-side comparative table formatter for `estimate --diff` displaying Old vs New metrics with colorized delta columns.

## Background
When comparing two WASM builds or two snapshots, viewing Old, New, and Delta in a single side-by-side table is much easier to review than two separate sequential tables.

## Acceptance criteria
- [ ] Renders 4-column comparison table: `Resource | Old | New | Change (+/- %)`
- [ ] Covers: WASM Size, CPU Instructions, RAM Bytes, Read Entries, Write Entries, Read Bytes, Write Bytes, Total Fee
- [ ] Colorizes increases in Red and decreases in Green
- [ ] Provides clean JSON structure for `--json` mode
- [ ] Tests verify table layout across terminal widths

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/report-side-by-side-diff`

Key files:
- `src/report/cost_report.rs`
- `src/config_snapshot/diff.rs`

Create `format_cost_report_diff(old: &CostReport, new: &CostReport) -> String` using comfy-table with aligned columns.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- 3-way diff comparison
- Interactive side-by-side scrolling
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# CACHING, PERSISTENCE & STORAGE (#93–102)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "feat(cache): implement LRU eviction for cache disk space management" "enhancement, performance, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Implement Least-Recently-Used (LRU) eviction with configurable max cache size (default: 50MB / 10,000 entries) to prevent unbounded disk growth.

## Background
The local simulation cache writes JSON files keyed by contract hash, function name, and arguments. Over months of CI builds or continuous development, millions of temporary test contracts can cause unbounded disk accumulation. An LRU eviction policy keeps cache size bounded.

## Acceptance criteria
- [ ] Maintains total cache size under configured threshold (default: 50 MB / 10,000 files)
- [ ] Evicts oldest accessed entries when limit is exceeded during `save_estimate`
- [ ] `--max-cache-size-mb <N>` config option supported
- [ ] Thread-safe and multi-process safe eviction logic
- [ ] Tests verify oldest entries are deleted when quota is reached

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-lru-eviction`

Key files:
- `src/cache.rs`
- `src/cli.rs`
- `src/main.rs`

In `src/cache.rs`, track access times or file metadata mtime. When `save_estimate` runs and total directory size exceeds quota, sort files by `mtime` ascending and delete oldest until under 90% of quota.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom distributed cache backends (Redis/Memcached)
- Filesystem quota integration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add SQLite-backed cache storage backend" "enhancement, storage, complexity: high, Stellar Wave" "$(cat <<'EOF'
## Summary
Provide an optional or default SQLite-backed cache backend (`rusqlite`) for faster queries, atomic transactions, and index-accelerated lookups.

## Background
Storing thousands of individual JSON files on disk creates filesystem inode overhead and slow listing performance during `cache stats` and `history` queries. SQLite provides atomic transactions, indexed lookups by hash/network/timestamp, and compact single-file storage.

## Acceptance criteria
- [ ] Implements cache storage using SQLite database (`~/.soroban-cost-estimator/cache.db`)
- [ ] Indexes by `(wasm_hash, function_name, network, created_at)`
- [ ] Provides atomic reads and writes without file-locking races
- [ ] Migrates existing JSON cache files on first run transparently
- [ ] All existing cache operations (save, load, stats, prune) work seamlessly
- [ ] Benchmarks demonstrate faster lookup and listing performance

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-sqlite-backend`

Key files:
- `Cargo.toml`
- `src/cache.rs`
- `src/lib.rs`

Add `rusqlite = { version = "0.31", features = ["bundled"] }` to `Cargo.toml`. Define schema `CREATE TABLE estimates (...)`. Implement `CacheStore` trait with SQLite implementation.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Remote PostgreSQL database support
- Distributed replication
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add `config cache query` with search filters" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config cache query` subcommand to search and filter cached estimates by WASM hash, function name, network, fee range, or date.

## Background
Developers and auditors often need to search historical estimates (e.g. find all functions with fees > 100,000 stroops or search estimates from a specific contract hash).

## Acceptance criteria
- [ ] `config cache query [--wasm-hash <hash>] [--fn <name>] [--network <net>] [--min-fee <stroops>] [--max-fee <stroops>] [--since <date>]`
- [ ] Outputs matched estimates in a formatted table with timestamp, function, CPU, and fee
- [ ] Supports `--json` mode with array of matching cached estimates
- [ ] Returns empty result notice cleanly if no estimates match filters
- [ ] Tests cover combinations of filter arguments

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-query-command`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `Query` variant to `CacheAction` in `src/cli.rs`. Implement `query_cache(filter: CacheFilter) -> Vec<CachedEstimate>` in `src/cache.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Full-text search on contract bytecode
- Regular expression matching on arguments
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add automatic cache invalidation on WASM file modification" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Ensure cache lookups verify both WASM SHA-256 hash and file modification timestamp to prevent serving stale estimates during rapid development.

## Background
If a developer recompiles a contract with the same function signature, the WASM hash changes. However, if cache lookup keys don't properly isolate WASM hashes, collisions or stale entries could occur. Explicit validation guarantees fresh simulations.

## Acceptance criteria
- [ ] Cache key strictly includes SHA-256 hash of entire WASM binary
- [ ] Changing even a single byte in the WASM file produces a new cache key
- [ ] Validates hash before returning cached estimate
- [ ] Tests verify that modifying WASM invalidates previous cache hits

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-hash-invalidation`

Key files:
- `src/cache.rs`
- `tests/cache_tests.rs`

Ensure `derive_cache_key(wasm_hash, fn_name, args, network)` rigorously uses full 64-char hex SHA-256 hash.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Compiler build cache integration (sccache)
- Source code AST caching
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): enforce strict cross-network cache isolation" "bug, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Ensure cached simulation estimates and network config snapshots are strictly partitioned by network name and passphrase to prevent cross-network contamination.

## Background
Testnet and Mainnet have different state, different ledger sequence numbers, and potentially different protocol fee rates. A simulation cached on Testnet must never be returned for a Mainnet estimation request.

## Acceptance criteria
- [ ] Cache directory structure partitions by network: `~/.soroban-cost-estimator/cache/<network>/<hash>/`
- [ ] Estimates saved on `testnet` are never returned for `mainnet` or `futurenet` queries
- [ ] Custom `--rpc-url` endpoints generate a distinct network partition based on URL/passphrase
- [ ] Unit and integration tests assert complete cross-network cache isolation

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-network-isolation`

Key files:
- `src/cache.rs`
- `tests/cache_tests.rs`

Include network identifier in directory path in `cache_dir_for_network(network)`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Cross-network fee comparison tools
- Bridging cache data
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add cache warm-up command (`config cache warm`)" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config cache warm --wasm <file.wasm>` subcommand to simulate and pre-populate the cache with estimates for all exported functions in advance.

## Background
In CI/CD environments or before running interactive demos, warming the cache in advance ensures that subsequent commands execute with zero latency from local cache.

## Acceptance criteria
- [ ] `config cache warm --wasm <file.wasm> [--network <net>]` simulates all functions in the contract
- [ ] Populates cache entries for each function with default/inferred arguments
- [ ] Displays progress bar during warm-up
- [ ] Reports summary: 'Warmed X functions into cache for <wasm_hash>'
- [ ] Exits with 0 on completion

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-warm-cmd`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `Warm { wasm: PathBuf, #[arg(long)] network: Option<String> }` to `CacheAction`. Reuse `estimate_all` logic with cache write-through.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Distributed cache pre-fetching
- Warming all historical contracts on network
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add execution duration and simulation status metadata to cached entries" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Extend `CachedEstimate` schema with `execution_duration_ms`, `network_passphrase`, and `stellar_core_version` metadata.

## Background
When reading historical cache entries, having execution duration and Stellar Core version metadata enables performance regression analysis over time.

## Acceptance criteria
- [ ] `CachedEstimate` struct includes fields: `execution_duration_ms: u64`, `network_passphrase: Option<String>`, `core_version: Option<String>`
- [ ] Values are populated during simulation save
- [ ] Backward-compatible deserialization for older cache files (using `#[serde(default)]`)
- [ ] Fields are visible in `--json` output

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-extended-metadata`

Key files:
- `src/cache.rs`
- `src/report/cost_report.rs`

Update `CachedEstimate` struct in `src/cache.rs` with serde default attributes.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Collecting full host machine CPU/RAM telemetry
- Hardware benchmarking
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add cache TTL (time-to-live) configuration" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--cache-ttl <duration>` option (e.g. `24h`, `7d`) to treat cached estimates older than the TTL as stale and force re-simulation.

## Background
State on live networks changes over time (e.g. storage rent cycles, entry modifications). Setting a TTL ensures cached estimates are automatically refreshed after a configured period.

## Acceptance criteria
- [ ] `--cache-ttl <duration>` flag added (accepts human formats like `30m`, `2h`, `1d`, `7d`)
- [ ] If cached estimate age exceeds TTL, it is treated as a cache miss and re-simulated
- [ ] Default TTL is configurable in `config.toml` (default: 7 days)
- [ ] Tests verify cache hit for fresh entries and cache miss for expired TTL entries

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-ttl-support`

Key files:
- `src/cli.rs`
- `src/cache.rs`
- `src/main.rs`

Add `parse_duration` helper. In `load_estimate`, compare `entry.created_at + ttl < Utc::now()`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Background daemon purging expired TTL entries
- Dynamic TTL based on ledger velocity
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add cache schema versioning and migration logic" "enhancement, storage, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Introduce a `schema_version` field to cache entries and implement automatic migration logic for backward compatibility across crate releases.

## Background
As Soroban Protocol evolves and new cost metrics are added (e.g. state archival rent metrics), cache entry schemas will change. Explicit schema versioning prevents deserialization errors when upgrading the tool.

## Acceptance criteria
- [ ] Cache JSON files contain `"schema_version": 1`
- [ ] Deserializer checks `schema_version` and applies migration transformations if reading older versions
- [ ] Rejects incompatible future schema versions with clear upgrade prompt
- [ ] Unit tests verify migration from v0 (unversioned legacy) to v1

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-schema-versioning`

Key files:
- `src/cache.rs`
- `tests/cache_tests.rs`

Use serde untagged enum or custom deserializer in `src/cache.rs` to detect version and migrate legacy structures.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Two-way forward and backward schema synchronization
- Binary flatbuffers serialization
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(cache): add `config cache verify` command to check cache integrity" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a `config cache verify` subcommand that scans all cache files on disk, verifies JSON validity, and quarantines or removes corrupted files.

## Background
Unexpected system crashes or power outages during file writes can leave truncated JSON files in the cache directory, causing deserialization errors on future runs. A verify command repairs corrupted cache states.

## Acceptance criteria
- [ ] `config cache verify` iterates through all cache files
- [ ] Checks JSON syntax validity and schema conformance
- [ ] Reports summary: 'Verified X cache files: Y valid, Z corrupted'
- [ ] `--repair` / `--fix` flag automatically deletes corrupted files
- [ ] Exits with 0 if clean, 1 if corrupted entries found

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/cache-verify-cmd`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/cache.rs`

Add `Verify { #[arg(long)] repair: bool }` to `CacheAction`. Read and parse every file in `cache_dir()`, removing invalid files if `repair` is set.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Cryptographic Merkle tree verification
- Filesystem bit rot recovery
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# CONFIG SNAPSHOT & DRIFT DETECTION (#103–112)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "feat(config): add automatic snapshot on network protocol upgrade detection" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Detect network protocol version bumps during RPC queries and automatically create an annotated snapshot marking the protocol transition.

## Background
When Stellar validators vote and upgrade the network protocol (e.g. Protocol 20 -> Protocol 21 -> Protocol 22), pricing curves and fee limits change fundamentally. Automatically capturing a snapshot at the moment of upgrade preserves the exact baseline before and after the transition.

## Acceptance criteria
- [ ] Compares `network_protocol_version` returned by RPC against latest saved snapshot
- [ ] If protocol version increased, creates a snapshot tagged with `protocol_upgrade_v{version}`
- [ ] Logs notice to user: '🎉 Stellar Protocol Upgrade detected (vX -> vY). Created snapshot: <path>'
- [ ] Generates automatic diff against previous protocol version
- [ ] Tests verify detection logic on protocol version change

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-protocol-upgrade-snapshot`

Key files:
- `src/rpc/config.rs`
- `src/config_snapshot/mod.rs`
- `src/main.rs`

In `src/rpc/config.rs`, track `protocol_version`. If different from previous snapshot, call `save_snapshot` with upgrade annotation.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Voting on validator protocol upgrades
- Simulating upcoming unreleased protocol versions
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add snapshot history query with timestamp navigation (`config snapshot show`)" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `config snapshot show [--at <timestamp> | --latest | <filename>]` to display the complete configuration settings of a specific historical snapshot.

## Background
Users want to view the exact fee parameters, instruction limits, and ledger costs that were active on a specific date in the past without manually opening JSON files.

## Acceptance criteria
- [ ] `config snapshot show <filename>` or `config snapshot show --latest [--network <net>]`
- [ ] Outputs formatted table of all configuration settings organized by category
- [ ] Supports `--json` mode for full structured JSON output
- [ ] Displays metadata: Timestamp, Ledger Sequence, Protocol Version, Network
- [ ] Returns error if requested snapshot is not found

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-snapshot-show`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/store.rs`

Add `Show { snapshot: Option<String>, #[arg(long)] latest: bool, #[arg(long)] network: Option<String>, #[arg(long)] json: bool }` to `ConfigAction`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Interactive snapshot timeline slider
- Editing historical snapshot values
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add `config diff --against-previous` for consecutive snapshot comparison" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--against-previous` flag to `config diff` to diff the latest snapshot against the snapshot immediately preceding it on disk.

## Background
Instead of diffing the live network against the latest snapshot, users frequently want to review the changes between the last two saved snapshots on disk without contacting the live network.

## Acceptance criteria
- [ ] `config diff --against-previous [--network <net>]` finds the two most recent snapshots on disk
- [ ] Computes and renders the diff table between snapshot N-1 and snapshot N
- [ ] Error with helpful message if fewer than 2 snapshots exist for the network
- [ ] Supports `--json` output format
- [ ] Tests cover 0, 1, and 2+ snapshot scenarios

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-diff-against-previous`

Key files:
- `src/cli.rs`
- `src/main.rs`
- `src/config_snapshot/store.rs`
- `src/config_snapshot/diff.rs`

Add `against_previous: bool` in `ConfigAction::Diff`. In `cmd_config_diff`, if true, sort snapshots by timestamp, pick last two, and call `diff_snapshots`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- N-way snapshot diff matrix
- Git-style interactive merge tool
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add snapshot retention policy (`--retain <count|days>`)" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add automated retention management to keep only the last N snapshots or snapshots within the last D days.

## Background
Automated snapshot tools or watch processes can create hundreds of snapshot files. A retention policy keeps snapshot directories clean without requiring manual deletion.

## Acceptance criteria
- [ ] `config snapshot --retain <N>` keeps only the N most recent snapshots, deleting older ones
- [ ] `config snapshot prune --older-than <days>` cleans up stale snapshots
- [ ] Never deletes the latest snapshot regardless of age
- [ ] Logs count of pruned snapshots
- [ ] Tests verify retention pruning with mock snapshot lists

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-snapshot-retention`

Key files:
- `src/cli.rs`
- `src/config_snapshot/store.rs`
- `src/main.rs`

Implement `prune_snapshots(network, retain_count, retain_days)` in `src/config_snapshot/store.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Uploading deleted snapshots to cloud archive
- Custom cron daemon
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add snapshot export and import commands for team collaboration" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `config snapshot export <file.json>` and `config snapshot import <file.json>` to export and import network snapshot bundles.

## Background
Allows teams to share network state snapshots (e.g. custom testnet or private devnet settings) across team members or CI nodes.

## Acceptance criteria
- [ ] `config snapshot export [--network <net>] --output <bundle.json>` exports snapshots
- [ ] `config snapshot import <bundle.json>` imports snapshots into local store
- [ ] Validates bundle integrity and ignores duplicate existing snapshots
- [ ] Prints summary of imported snapshot files
- [ ] Tests assert export and import roundtrip

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-snapshot-export-import`

Key files:
- `src/cli.rs`
- `src/config_snapshot/store.rs`
- `src/main.rs`

Add `Export` and `Import` subcommands under `ConfigAction`. Read/write snapshot files to bundle archive.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Encrypted snapshot export
- Decentralized IPFS distribution
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add config setting value explanations and unit annotations" "enhancement, ux, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Include human-readable unit descriptions for configuration values (e.g. 'stroops per 10,000 CPU instructions', 'stroops per 1KB storage write') in diff and show outputs.

## Background
Stellar config settings represent fee rates scaled by specific increments (e.g. per 10,000 instructions or per 1,024 bytes). Showing units eliminates confusion over what the raw numbers mean.

## Acceptance criteria
- [ ] Adds unit description dictionary for all `ConfigSettingEntry` fields
- [ ] Displays unit descriptions as notes or table column in `config diff` and `config snapshot show`
- [ ] Includes `unit_description` in JSON setting schemas
- [ ] Tests verify descriptions exist for all compute and ledger cost fields

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-setting-units`

Key files:
- `src/config_snapshot/model.rs`
- `src/config_snapshot/diff.rs`

Create `pub fn setting_unit_description(field_path: &str) -> Option<&'static str>` in `src/config_snapshot/model.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Currency unit conversion to fiat (USD/EUR)
- Dynamic unit re-scaling
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add `config diff --pricing-only` filter flag" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--pricing-only` flag to `config diff` to filter out non-pricing changes (e.g. contract max size, key limits) and display only fee-rate adjustments.

## Background
Protocol upgrades often adjust dozens of structural parameters (e.g. validator keys, expiry bounds). Contract financial auditors only care about fee-rate changes. Filtering out non-pricing noise focuses attention on fee impact.

## Acceptance criteria
- [ ] `config diff --pricing-only` hides all rows where `is_pricing_change == false`
- [ ] If only non-pricing changes occurred, displays: 'No pricing changes detected (X non-pricing changes omitted)'
- [ ] Supported in both table and `--json` formats
- [ ] Tests verify filtering logic

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-diff-pricing-only`

Key files:
- `src/cli.rs`
- `src/config_snapshot/diff.rs`
- `src/main.rs`

Add `#[arg(long)] pricing_only: bool` to `ConfigAction::Diff`. In `src/config_snapshot/diff.rs`, filter `changes` vector where `c.is_pricing_change`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom regex field filtering
- Interactive column selector
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add configurable config change notification thresholds" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `--threshold-percent <N>` (e.g. `--threshold-percent 10`) to only alert or fail if a pricing parameter changes by more than N percent.

## Background
Minor fee fluctuations (e.g. 1-2% fee rate tweaks) may not warrant breaking CI builds or triggering urgent alerts. Configurable percentage thresholds allow teams to ignore minor drift and only trigger on major rate hikes.

## Acceptance criteria
- [ ] `--threshold-percent <N>` flag added to `config diff` and `watch`
- [ ] Flags changes where `abs(new_val - old_val) / old_val * 100.0 >= threshold`
- [ ] Exits with 0 if all changes are below threshold; exits with 1 if changes exceed threshold
- [ ] Highlights exceeding changes in bold red in table output
- [ ] Tests verify threshold boundary comparisons

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-threshold-percent`

Key files:
- `src/cli.rs`
- `src/config_snapshot/diff.rs`
- `src/main.rs`

Add `threshold_percent: Option<f64>` to `ConfigAction::Diff`. Filter and evaluate significance in `ConfigDiff::has_significant_pricing_changes(threshold)`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Machine-learning anomaly detection
- Multi-variable statistical significance tests
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add `config snapshot --validate` to check snapshot schema validity" "enhancement, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a validation check to verify that saved snapshot files conform to current Stellar Protocol XDR schema definitions.

## Background
Ensures that snapshot files saved from previous protocol versions remain readable and deserialize correctly into current struct definitions.

## Acceptance criteria
- [ ] `config snapshot validate [<filename> | --all]` command
- [ ] Checks JSON syntax, required fields, and XDR deserialization compatibility
- [ ] Reports valid/invalid status for each file
- [ ] Returns exit code 0 if all valid, 1 if any invalid
- [ ] Tests cover valid and malformed snapshot files

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-snapshot-validate`

Key files:
- `src/cli.rs`
- `src/config_snapshot/store.rs`
- `src/main.rs`

Add `Validate` variant to `ConfigAction`. Deserialize each snapshot into `ConfigSnapshot` and return errors.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Fixing corrupted XDR bytes automatically
- Online schema validation registry
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(config): add config setting change history tracking log" "enhancement, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Generate a historical timeline log showing when each specific config setting was modified across all saved snapshots.

## Background
When auditing past fee increases over a 12-month period, developers need a chronological timeline of when `fee_rate_per_instructions_increment` or `fee_write_1kb` changed and by how much.

## Acceptance criteria
- [ ] `config history [--setting <name>] [--network <net>]` command
- [ ] Builds chronological timeline across all historical snapshots
- [ ] Displays: Date, Ledger, Setting Field, Old Value, New Value, Delta Percentage
- [ ] `--json` outputs structured timeline array
- [ ] Tests verify timeline assembly from multiple mock snapshots

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/config-history-log`

Key files:
- `src/cli.rs`
- `src/config_snapshot/store.rs`
- `src/config_snapshot/diff.rs`
- `src/main.rs`

Add `History` command. Iterate sorted snapshots pairwise, aggregate diffs, and filter by setting name if requested.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Exporting to PDF audit reports
- Git blame integration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# TESTING & QUALITY ASSURANCE (#113–127)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "test: add end-to-end integration tests for all CLI commands" "testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add comprehensive integration tests covering `estimate`, `estimate-all`, `config snapshot`, `config diff`, `completions`, and error flag combinations.

## Background
Unit tests cover isolated modules, but end-to-end integration tests using `assert_cmd` ensure that CLI arguments, exit codes, stdin/stdout piping, and flag interactions work reliably across binary builds.

## Acceptance criteria
- [ ] Adds `assert_cmd` and `predicates` dev-dependencies
- [ ] Tests CLI execution of all subcommands with valid arguments and flags
- [ ] Tests invalid arguments (missing wasm, bad network name, invalid number) verifying exit code and error output
- [ ] Tests `--json` and table outputs for each command
- [ ] Runs reliably in CI offline using test fixtures

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/cli-e2e-suite`

Key files:
- `Cargo.toml`
- `tests/cli_tests.rs`

Use `assert_cmd::Command::cargo_bin("soroban-cost-estimator")` to execute binary commands against `tests/fixtures/contract.wasm`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Testing live internet connections in unit CI jobs (use mock/fixtures)
- GUI automated testing
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add property-based tests for fee calculator using `proptest`" "testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add property-based fuzz testing for `fee_calc.rs` to verify mathematical invariants: non-negative refundable fees, total fee >= sum of parts, and overflow resistance.

## Background
Fee calculation involves complex ceiling divisions and resource fee rate scaling. Property testing with randomized inputs guarantees that arithmetic overflow never occurs and fee invariants hold across all possible u32/i64 input ranges.

## Acceptance criteria
- [ ] Adds `proptest` dev-dependency
- [ ] Verifies invariant: `refundable_fee >= 0` for all valid input combinations
- [ ] Verifies invariant: `total_fee == non_refundable_fee + refundable_fee`
- [ ] Verifies invariant: no arithmetic panic/overflow on extreme inputs (e.g. u32::MAX instructions, u64::MAX bytes)
- [ ] Runs in standard `cargo test` suite

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/fee-calc-proptest`

Key files:
- `Cargo.toml`
- `src/report/fee_calc.rs`
- `tests/fee_calc_tests.rs`

Write `proptest!` blocks generating random `ResourceCosts` and `NetworkConfig` structs, asserting mathematical invariants.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Formal mathematical verification proofs (Coq/Isabelle)
- Fuzzing WASM parser with AFL
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add mock Soroban RPC server fixture for offline deterministic testing" "testing, network, complexity: high, Stellar Wave" "$(cat <<'EOF'
## Summary
Create a local mock HTTP server fixture (`wiremock` or `mockito`) that simulates Soroban JSON-RPC responses for deterministic offline test suites.

## Background
Live RPC testing against public Testnet is subject to network flakiness, rate limits, and network latency. A mock RPC server enables fast, deterministic integration tests that run completely offline in under 1 second.

## Acceptance criteria
- [ ] Adds `wiremock` dev-dependency
- [ ] Mocks JSON-RPC endpoints: `simulateTransaction`, `getLedgerEntries`, `getHealth`, `getNetwork`
- [ ] Provides standard mocked responses for: upload simulation, invocation simulation, degraded fee rates, and 429 rate limits
- [ ] Integrates into `cargo test` without requiring internet connectivity
- [ ] Verifies error handling when mock returns simulated RPC errors

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/mock-rpc-fixture`

Key files:
- `Cargo.toml`
- `tests/mock_rpc_tests.rs`
- `tests/common/mod.rs`

Use `wiremock::MockServer` to mount JSON-RPC matchers and verify client responses against localhost URL.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Embedding full standalone `stellar-core` C++ binary in tests
- Docker container orchestration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add performance benchmarks for WASM parsing with `criterion`" "testing, performance, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add Criterion.rs micro-benchmarks for WASM loading, section parsing, SHA-256 hashing, and contract spec decoding.

## Background
Ensures that future enhancements to WASM inspection and type validation do not introduce performance regressions when parsing large contract binaries.

## Acceptance criteria
- [ ] Adds `criterion` dev-dependency and bench target in `Cargo.toml`
- [ ] Benchmarks: WASM binary hashing, `contractspecv0` decoding, and section traversal
- [ ] Measures throughput (MB/s) and latency (µs)
- [ ] Runs with `cargo bench --bench wasm_benchmark`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/wasm-criterion-benchmarks`

Key files:
- `Cargo.toml`
- `benches/wasm_bench.rs`

Create `benches/wasm_bench.rs` using `criterion::criterion_group!` measuring `parse_wasm()` against `fixtures/contract.wasm`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Continuous cloud benchmark dashboard setup
- Flamegraph generation tool
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add snapshot tests for all table and report formats" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add snapshot regression tests (`insta`) to lock down output formatting across table, JSON, Markdown, and CSV reports.

## Background
Formatting changes can accidentally break alignment, truncate columns, or alter JSON schemas. Snapshot testing ensures that any change to output formatting is intentional and reviewable in PR diffs.

## Acceptance criteria
- [ ] Adds `insta` dev-dependency
- [ ] Captures snapshots of: `estimate` table, `estimate --json`, `estimate-all` table, `config diff` table, `config snapshot show`
- [ ] Verifies exact match against golden snapshot files
- [ ] Runs cleanly in CI test suite

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/insta-snapshot-tests`

Key files:
- `Cargo.toml`
- `tests/snapshots/`
- `tests/report_snapshot_tests.rs`

Use `insta::assert_snapshot!` on formatted report strings from `src/report/`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Visual screenshot comparison of terminal renders
- GUI snapshots
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add comprehensive edge-case tests for `xlm_to_stroops` string parsing" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Expand unit tests for `xlm_to_stroops` and `stroops_to_xlm` covering all decimal precision boundaries, whitespace, signs, and overflow inputs.

## Background
Currency parsing is critical to financial accuracy. Subtle floating-point errors or rounding issues must be completely eliminated through exhaustive boundary unit testing.

## Acceptance criteria
- [ ] Tests string inputs: `"0"`, `"0.1"`, `"0.0000001"` (1 stroop), `"1.0000000"`, `"100.500"`
- [ ] Tests error cases: `""`, `"-5"`, `"1.00000001"` (sub-stroop precision error), `"abc"`, `"1..0"`, `" 1.0 "`
- [ ] Tests overflow boundaries: `i64::MAX` stroops
- [ ] All tests pass with 100% branch coverage on conversion helpers

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/xlm-parsing-edge-cases`

Key files:
- `src/report/fee_calc.rs`
- `tests/fee_calc_tests.rs`

Add extensive test matrix in `#[cfg(test)]` module in `src/report/fee_calc.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Multi-currency conversion logic
- Locale-based comma decimals
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add unit tests for `parse_interval_secs` with all duration suffixes" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add exhaustive unit tests for interval duration parsing (`s`, `m`, `h`, `d` suffixes) covering edge cases and invalid formats.

## Background
Used by `watch`, `--timeout`, and retention flags to parse human time intervals. Ensuring robust parsing prevents runtime panics on malformed input.

## Acceptance criteria
- [ ] Tests valid suffixes: `"10s"` -> 10, `"5m"` -> 300, `"2h"` -> 7200, `"1d"` -> 86400
- [ ] Tests raw numbers without suffix (e.g. `"60"` -> 60s)
- [ ] Tests error cases: `"0s"` (zero duration), `"-10s"`, `"5x"`, `""`, `"s"`
- [ ] Unit tests achieve full branch coverage

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/interval-parser-tests`

Key files:
- `src/cli.rs`
- `tests/cli_tests.rs`

Add unit tests in `src/cli.rs` testing `parse_interval_secs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- ISO 8601 calendar duration parsing
- Leap second handling
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add error propagation and exit code verification tests" "testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Verify that every variant of `AppError` is correctly propagated, formatted into user-friendly error text, and maps to the expected process exit code.

## Background
Errors should never panic or print raw Rust debug traces (`unwrap()`), but instead emit clear user-facing messages on stderr with deterministic non-zero exit codes.

## Acceptance criteria
- [ ] Tests each `AppError` variant (WasmNotFound, InvalidSpec, RpcFailure, ConfigSettingNotFound, etc.)
- [ ] Asserts that error output contains helpful diagnostic text and does not leak raw stack traces
- [ ] Asserts standard exit code mapping (1 for general error, 2 for CLI usage error)
- [ ] Tests verify no panics occur on any error path

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/error-propagation-suite`

Key files:
- `src/error.rs`
- `tests/cli_tests.rs`

Write integration test suite triggering each error condition and checking stderr and exit status.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Generating automated core dumps
- Crash reporting telemetry
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add concurrent cache access and thread safety tests" "testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Test concurrent read and write access to the simulation cache across multiple parallel threads and async tasks to guarantee data integrity.

## Background
When `estimate-all` runs parallel simulations or multiple CLI processes run simultaneously, concurrent writes to the same cache directory must not corrupt files or trigger race conditions.

## Acceptance criteria
- [ ] Spawns 50 concurrent async tasks writing and reading cache entries simultaneously
- [ ] Verifies no corrupted JSON files or partial writes are produced
- [ ] Verifies all reads return valid deserialized data
- [ ] Tests pass reliably under `cargo test -- --test-threads=8`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/concurrent-cache-safety`

Key files:
- `tests/cache_tests.rs`
- `src/cache.rs`

Use `tokio::spawn` and `futures::future::join_all` to execute concurrent cache reads/writes with tempdir.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Multi-node distributed filesystem testing (NFS/CIFS)
- POSIX lock performance benchmarking
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add config diff test suite covering all configuration setting permutations" "testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Create comprehensive unit and snapshot tests for `diff_snapshots` covering all combinations of present, missing, increased, decreased, and identical config settings.

## Background
Stellar configuration contains nested structures (`ConfigSettingContractComputeV0`, `ConfigSettingContractLedgerCostV0`, `ConfigSettingContractHistoricalDataV0`, `ConfigSettingContractEventsV0`). Testing every branch ensures diffing is 100% accurate.

## Acceptance criteria
- [ ] Tests diffing when: no fields change, only pricing fields change, only non-pricing fields change, fields are added/removed between versions
- [ ] Tests positive, negative, and zero percentage calculations
- [ ] Verifies `has_pricing_changes()` accurately reflects changes
- [ ] Test suite covers all `ConfigSettingId` variants

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/config-diff-permutations`

Key files:
- `tests/config_diff_tests.rs`
- `src/config_snapshot/diff.rs`

Write test cases with synthetic `ConfigSnapshot` fixtures covering all diff branches.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Fuzzing invalid XDR binary streams
- Dynamic validator governance simulation
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add XDR encoding/decoding roundtrip validation tests" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Verify roundtrip serialization and deserialization of all custom XDR structures, `ScVal` representations, and transaction envelopes.

## Background
Soroban relies on XDR (External Data Representation). Roundtrip testing (`ScVal -> bytes/base64 -> ScVal`) guarantees that encoding transformations never lose precision or corrupt data types.

## Acceptance criteria
- [ ] Tests roundtrip encoding for all `ScVal` types (Bool, I32, I64, U32, U64, Symbol, String, Vec, Map, Address)
- [ ] Tests `SorobanTransactionData` encoding and decoding
- [ ] Tests base64 encoding and decoding helpers in `src/xdr_helper.rs`
- [ ] Asserts byte-level equality on roundtrip

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/xdr-roundtrip-suite`

Key files:
- `src/xdr_helper.rs`
- `tests/xdr_tests.rs`

Implement parameterized tests in `tests/xdr_tests.rs` asserting `decode(encode(val)) == val`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom XDR schema compiler
- ASN.1 parsing
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add CLI help text and argument parsing assertion tests" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add automated tests using `clap::Command::debug_assert` to verify CLI command definitions, argument aliases, conflicts, and help text completeness.

## Background
`clap` provides built-in validation methods to ensure argument definitions have no naming conflicts, missing required arguments, or broken subcommands.

## Acceptance criteria
- [ ] Calls `Cli::command().debug_assert()` in a unit test
- [ ] Verifies all commands and flags have non-empty help descriptions
- [ ] Asserts short flag uniqueness across global and subcommand scopes
- [ ] Runs on every `cargo test` invocation

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/cli-debug-assert`

Key files:
- `src/cli.rs`
- `tests/cli_tests.rs`

Add test `#[test] fn verify_cli() { use clap::CommandFactory; Cli::command().debug_assert(); }` in `tests/cli_tests.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Spell-checking help text strings
- Grammar linting
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add test suite for `config_snapshot::store::list_snapshots` filtering" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add comprehensive unit tests for snapshot file discovery, sorting by timestamp, network filtering, and handling empty directories.

## Background
The snapshot store module scans directories for files matching `network-<timestamp>.json`. Rigorous tests ensure correct sorting and network isolation even with mixed filenames or malformed file names.

## Acceptance criteria
- [ ] Tests listing snapshots across multiple network subdirectories/prefixes
- [ ] Tests correct chronological sorting (newest first)
- [ ] Tests handling of non-JSON files or foreign files in the snapshot directory (gracefully ignored)
- [ ] Tests empty directory behavior
- [ ] Tests run against isolated temporary directory fixtures

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/snapshot-store-filtering`

Key files:
- `src/config_snapshot/store.rs`
- `tests/config_diff_tests.rs`

Create temp directory with mock snapshot files and verify `list_snapshots` returns expected filtered and sorted `PathBuf` vectors.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Cloud storage bucket listing
- Symlink loop resolution
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add integration test for `watch` command graceful shutdown on SIGINT" "testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Write an integration test that starts `watch` in a background subprocess, sends `SIGINT` (Ctrl-C), and verifies clean exit with code 0.

## Background
Ensures that signal handlers in `watch` mode properly catch terminal interrupt signals, abort pending sleep/poll loops, and exit cleanly without leaving orphaned processes or corrupted partial files.

## Acceptance criteria
- [ ] Integration test spawns `soroban-cost-estimator watch --network testnet` subprocess
- [ ] Waits for first poll line output, then sends `libc::SIGINT` / `nix::sys::signal::SIGINT`
- [ ] Asserts process terminates within 2 seconds with exit status 0
- [ ] Verifies no corrupted snapshot files remain on disk
- [ ] Runs cleanly on Linux and macOS CI runners

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/watch-sigint-shutdown`

Key files:
- `tests/cli_tests.rs`
- `src/main.rs`

Use `std::process::Command` to spawn child process, read stdout line, send signal with `nix::sys::signal::kill`, and assert `status.success()`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Windows Ctrl-Break signal testing (platform-specific)
- Kernel kill -9 recovery
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "test: add regression test for negative refundable fee edge cases with storage I/O" "testing, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add specific unit and regression tests verifying that refundable fee calculations never underflow or become negative when storage fees exceed total fees.

## Background
Historical fee calculation bug caused negative refundable fee values when non-refundable storage fees were subtracted from total fees under certain edge-case fee rate configurations. Adding a permanent regression test guarantees this bug never re-occurs.

## Acceptance criteria
- [ ] Tests exact fee calculation inputs that previously caused negative refundable fee
- [ ] Tests extreme storage I/O cases (large write bytes, high write fee rate)
- [ ] Asserts `refundable_fee >= 0` and total fee matches sum of parts
- [ ] Tests run as part of `cargo test`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b test/refundable-fee-regression`

Key files:
- `src/report/fee_calc.rs`
- `tests/fee_calc_tests.rs`

Add regression test case in `tests/fee_calc_tests.rs` with documented historical edge-case inputs.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Modifying protocol fee equations in Stellar Core
- Simulating future protocol math
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# DOCUMENTATION & GUIDES (#128–137)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "docs: add comprehensive CLI command reference documentation" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/commands/reference.md` providing an exhaustive reference of all CLI commands, subcommands, global flags, and environment variables with concrete examples.

## Background
While individual command docs exist, developers need a single, unified reference page listing every command, flag, default value, environment variable, and exit code for quick reference.

## Acceptance criteria
- [ ] Creates `docs/commands/reference.md` covering `estimate`, `estimate-all`, `config snapshot`, `config diff`, `watch`, `completions`, and `rpc`
- [ ] Includes tables with: Flag name, Short alias, Default value, Description, and Example usage
- [ ] Documents exit codes (0 = success, 1 = error/pricing change, 2 = usage error)
- [ ] Linked from `docs/SUMMARY.md` and main `README.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/command-reference`

Key files:
- `docs/commands/reference.md`
- `docs/SUMMARY.md`
- `README.md`

Structure documentation with markdown tables, code snippet blocks, and CLI invocation examples.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Manpage roff generation
- Interactive online CLI simulator
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add architecture overview document with data flow diagrams" "documentation, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/architecture.md` explaining internal subsystems (WASM parser, RPC client, XDR helpers, fee calculator, snapshot diff engine, cache) with Mermaid diagrams.

## Background
New contributors need a clear mental model of how data flows from user CLI input -> WASM parsing -> `simulateTransaction` RPC -> XDR decoding -> Fee calculation -> Cache store -> Terminal/JSON rendering.

## Acceptance criteria
- [ ] Documents the 6 core architectural layers of `soroban-cost-estimator`
- [ ] Includes Mermaid flowcharts showing the simulation pipeline and config snapshot drift detection pipeline
- [ ] Explains key design invariants (e.g. integer arithmetic for stroops, unwrap rules, separation of non-refundable and refundable fees)
- [ ] Linked in `CONTRIBUTING.md` and `docs/SUMMARY.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/architecture-overview`

Key files:
- `docs/architecture.md`
- `docs/SUMMARY.md`
- `CONTRIBUTING.md`

Write clean Markdown with Mermaid sequence and flowchart diagrams explaining module responsibilities.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Video architecture walkthrough
- UML class diagrams
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add troubleshooting guide for common RPC, WASM, and network errors" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/troubleshooting.md` documenting common error messages, root causes, and step-by-step resolution steps.

## Background
Developers encountering errors like `HostError`, `TransactionSimulationFailed`, `ConfigSettingNotFound`, or RPC connection timeouts need an actionable troubleshooting guide to resolve issues quickly.

## Acceptance criteria
- [ ] Documents solutions for common errors: `HostError (Error(Contract, #X))` (contract panics), `TransactionSimulationFailed` (footprint/authorization missing), `ConfigSettingNotFound` (protocol version mismatch), `429 Too Many Requests` (RPC rate limiting), `InvalidSpec` (WASM missing spec section)
- [ ] Provides debugging tips using `--verbose` and `RUST_LOG=debug`
- [ ] Includes FAQ section for common setup questions
- [ ] Linked from `README.md` and `docs/SUMMARY.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/troubleshooting-guide`

Key files:
- `docs/troubleshooting.md`
- `docs/SUMMARY.md`
- `README.md`

Organize by error symptom -> cause -> solution with concrete bash snippet fixes.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated issue reporting bot
- Live chat support widget
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add formal CHANGELOG.md following Keep a Changelog standard" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Create a `CHANGELOG.md` following the Keep a Changelog specification and Semantic Versioning to document all past and upcoming releases.

## Background
A standardized changelog helps users, package maintainers, and downstream tools track features, bug fixes, breaking changes, and upgrade paths between crate versions.

## Acceptance criteria
- [ ] Creates `CHANGELOG.md` following [Keep a Changelog 1.1.0](https://keepachangelog.com/)
- [ ] Documents `[Unreleased]` section and historical `[0.1.0]` release notes
- [ ] Categorizes changes under: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`
- [ ] Linked from `README.md` and `Cargo.toml`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/changelog-setup`

Key files:
- `CHANGELOG.md`
- `README.md`
- `Cargo.toml`

Review git history and roadmap to compile comprehensive release notes for v0.1.0 and unreleased items.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated Git cliff CI configuration (see #139)
- Blog post generation
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add migration guide from Stellar CLI (`stellar contract invoke --cost`)" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/migration-guide.md` explaining how `soroban-cost-estimator` complements and extends native `stellar-cli` commands.

## Background
Developers familiar with `stellar contract invoke --cost` or `stellar-cli` need a side-by-side guide showing feature comparisons, syntax equivalents, and why config drift tracking matters.

## Acceptance criteria
- [ ] Creates `docs/migration-guide.md` with comparison table (`stellar-cli` vs `soroban-cost-estimator`)
- [ ] Demonstrates equivalent commands for contract upload, function invocation, and batch estimation
- [ ] Explains unique capabilities: versioned config snapshots, drift detection, staleness invalidation, and CI diff exits
- [ ] Linked from `docs/SUMMARY.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/migration-guide`

Key files:
- `docs/migration-guide.md`
- `docs/SUMMARY.md`
- `README.md`

Provide side-by-side bash command examples and output comparisons.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Direct wrapper invoking `stellar-cli` binary
- Stellar CLI plugins
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add CI/CD integration guide with copy-pasteable GitHub Actions workflows" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/ci-cd-integration.md` with complete GitHub Actions, GitLab CI, and Docker examples for continuous contract gas monitoring.

## Background
Teams want to run cost estimation in pull requests and fail builds on fee regressions or network pricing changes. Ready-to-use CI workflow snippets accelerate team adoption.

## Acceptance criteria
- [ ] Creates `docs/ci-cd-integration.md`
- [ ] Includes complete GitHub Actions workflow for PR cost estimation comments
- [ ] Includes GitHub Actions workflow for weekly network drift detection (`config diff`)
- [ ] Includes GitLab CI `.gitlab-ci.yml` snippet and Dockerfile usage guide
- [ ] Linked in `docs/SUMMARY.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/cicd-integration-guide`

Key files:
- `docs/ci-cd-integration.md`
- `docs/SUMMARY.md`

Provide tested YAML workflow definitions using `actions/checkout`, `actions-rs/cargo`, and PR comment actions.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Custom GitHub Marketplace Action repository
- SaaS dashboard integration
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add network configuration and RPC provider reference directory" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/networks.md` detailing all supported networks (Testnet, Mainnet, Futurenet, Local), default endpoints, passphrases, and known RPC providers.

## Background
Developers often need to know which RPC URLs, network passphrases, and friendbot endpoints correspond to each network, as well as how to configure custom private endpoints.

## Acceptance criteria
- [ ] Creates `docs/networks.md` listing: Network Name, Default RPC URL, Network Passphrase, Chain ID, Protocol Version, Public vs Private Provider notes
- [ ] Documents configuring custom RPCs and setting API keys via `--header` or `config.toml`
- [ ] Documents local standalone setup with `stellar/quickstart` Docker container
- [ ] Linked in `docs/SUMMARY.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/networks-reference`

Key files:
- `docs/networks.md`
- `docs/SUMMARY.md`

Structure documentation with markdown tables and configuration examples.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Maintaining an uptime dashboard for external RPC providers
- Hosting public RPC nodes
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add detailed Soroban fee calculation methodology document" "documentation, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `docs/concepts/fee-calculation.md` breaking down the mathematical formulas for CPU, Storage Read/Write, Transaction Size, Base Fee, and Storage Rent.

## Background
Soroban's fee model uses a two-tier fee structure: non-refundable resource fees (consumed during simulation) and refundable resource fees (storage rent, write footprints). Documenting the exact math creates transparency and trust.

## Acceptance criteria
- [ ] Documents mathematical equations for: CPU instruction cost scaling, Storage entry read/write fee calculation, Storage byte read/write fee calculation, Transaction envelope size fee, Base transaction inclusion fee, Refundable storage rent fee
- [ ] References specific `stellar-xdr` config setting structures and fields
- [ ] Explains why fee estimates match Stellar Core `simulateTransaction` results exactly
- [ ] Linked in `docs/SUMMARY.md` and `docs/concepts/resource-fees.md`

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/fee-calculation-methodology`

Key files:
- `docs/concepts/fee-calculation.md`
- `docs/SUMMARY.md`
- `src/report/fee_calc.rs`

Include LaTeX / KaTeX formatted math equations explaining the scaling constants and ceiling integer arithmetic.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Interactive calculator web application
- Designing new fee models for future protocols
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: add Rustdoc API documentation for library crate consumers" "documentation, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add comprehensive `///` docstrings, module documentation, and `# Examples` across all public items in `src/lib.rs`, `src/rpc/`, `src/report/`, `src/wasm/`, and `src/config_snapshot/`.

## Background
When other Rust tools and crates import `soroban-cost-estimator` as a library, high-quality documentation on structs, enums, and functions with runnable examples on docs.rs is essential.

## Acceptance criteria
- [ ] All public structs, enums, traits, and functions have comprehensive doc comments
- [ ] Includes runnable or compile-checked `# Examples` in docs
- [ ] Enables `#![warn(missing_docs)]` in `src/lib.rs` without warnings
- [ ] `cargo doc --no-deps` builds cleanly without warnings

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/rustdoc-api-docs`

Key files:
- `src/lib.rs`
- `src/report/mod.rs`
- `src/wasm/mod.rs`
- `src/config_snapshot/mod.rs`
- `src/rpc/mod.rs`

Write doc comments explaining purpose, parameters, return types, errors, and usage code blocks.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Hosting private docs server
- Generating non-Rust language bindings
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "docs: enhance CONTRIBUTING.md with step-by-step development guide and PR checklist" "documentation, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Update `CONTRIBUTING.md` with complete local setup instructions, test execution commands, code style guidelines, and PR review checklist.

## Background
A clear, welcoming contribution guide helps community members and Drips Wave contributors submit clean, compliant PRs that pass CI on the first attempt.

## Acceptance criteria
- [ ] Updates `CONTRIBUTING.md` with prerequisites (Rust version, rustfmt, clippy, git)
- [ ] Documents commands for running unit tests, integration tests, and benchmarks
- [ ] Explains repository coding standards: no `unwrap()` in `src/`, integer stroop math, conventional commits
- [ ] Includes PR submission checklist

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b docs/contributing-improvements`

Key files:
- `CONTRIBUTING.md`
- `docs/contributing.md`

Ensure alignment with Drips Wave issue guidelines and repository workflow.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Setting up automated contributor reward bot
- Code of Conduct rewrite
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# CI/CD, DEVOPS & AUTOMATION (#138–142)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "ci: add mock RPC integration test pipeline to GitHub Actions" "ci, testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add a dedicated integration test job in `.github/workflows/ci.yml` that runs integration tests against the mock RPC server fixture on every push and PR.

## Background
Unit tests run fast, but verifying that the CLI binary interacts correctly with simulated RPC endpoints in CI guarantees zero regressions without relying on flaky live networks.

## Acceptance criteria
- [ ] Adds `integration-tests` job to `.github/workflows/ci.yml`
- [ ] Executes `cargo test --test mock_rpc_tests --test cli_tests`
- [ ] Runs completely offline without external network dependencies
- [ ] Fails build if any CLI command execution or RPC simulation fails
- [ ] Completes in under 2 minutes

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b ci/mock-rpc-pipeline`

Key files:
- `.github/workflows/ci.yml`

Add job matrix in `ci.yml` running cargo test suite with required check status.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Running live testnet RPC tests on every commit (subject to rate limits)
- Self-hosted runner setup
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "ci: add automated release workflow with multi-platform binary assets" "ci, devops, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Create `.github/workflows/release.yml` triggered on git tags (e.g. `v*`) that builds release binaries for Linux, macOS, and Windows and creates GitHub Releases.

## Background
Users who do not have a local Rust toolchain want precompiled standalone executable binaries for Linux x86_64/ARM64, macOS x86_64/ARM64 (Apple Silicon), and Windows.

## Acceptance criteria
- [ ] Triggered on tag push `v*.*.*`
- [ ] Cross-compiles optimized release binaries for: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`
- [ ] Generates SHA-256 checksums file (`checksums.txt`)
- [ ] Attaches compressed archives (.tar.gz / .zip) to GitHub Release automatically
- [ ] Publishes package to crates.io if `CRATES_IO_TOKEN` secret is configured

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b ci/release-automation`

Key files:
- `.github/workflows/release.yml`

Use `softprops/action-gh-release` and `taiki-e/upload-rust-binary-action` for multi-platform compilation and packaging.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Publishing Homebrew formula
- Debian/RPM package repositories
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "ci: add dependency audit and security vulnerability scanning" "ci, security, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Add `cargo audit` and `cargo deny` checks to CI to scan crate dependencies for known security advisories, unmaintained crates, and license compliance.

## Background
Maintaining a secure dependency tree ensures that cryptographic dependencies (e.g. `sha2`, `stellar-xdr`, `reqwest`, `base64`) are free of known vulnerabilities (CVEs) and compliant with MIT/Apache-2.0 licensing.

## Acceptance criteria
- [ ] Adds `security-audit` job to `.github/workflows/ci.yml`
- [ ] Runs `cargo audit` using RustSec Advisory Database
- [ ] Runs `cargo deny check bans licenses sources` with configuration in `deny.toml`
- [ ] Fails CI if a high-severity vulnerability or non-compliant license is introduced

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b ci/security-audit`

Key files:
- `.github/workflows/ci.yml`
- `deny.toml`

Use `rustsec/audit-check` or `EmbarkStudios/cargo-deny-action` in GitHub Actions.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Automated Dependabot auto-merge
- DAST dynamic penetration testing
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "ci: add multi-platform build matrix for Linux, macOS, and Windows" "ci, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Configure CI build matrix to run `cargo check` and `cargo test` across `ubuntu-latest`, `macos-latest`, and `windows-latest`.

## Background
Path separators (`/` vs `\`), filesystem permissions, and terminal escape code handling differ between Unix and Windows. Testing on all 3 major platforms prevents OS-specific bugs from slipping into releases.

## Acceptance criteria
- [ ] CI `build` job uses matrix: `os: [ubuntu-latest, macos-latest, windows-latest]`
- [ ] All unit and integration tests pass on all 3 platforms
- [ ] Ensures path resolution (`dirs::config_dir()`, `dirs::cache_dir()`) works cleanly on Windows and macOS
- [ ] Job completion time remains under 5 minutes

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b ci/cross-platform-matrix`

Key files:
- `.github/workflows/ci.yml`

Update `.github/workflows/ci.yml` with `strategy.matrix.os`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Building for mobile targets (iOS/Android)
- WebAssembly target compilation for browser
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "ci: add code coverage reporting with `cargo-tarpaulin` / `llvm-cov`" "ci, testing, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Add automated test coverage measurement in CI using `cargo-llvm-cov` or `cargo-tarpaulin` and upload coverage reports to Codecov.

## Background
Tracking test line and branch coverage ensures that new features and bug fixes include proper test coverage and prevents untested code paths.

## Acceptance criteria
- [ ] Adds `coverage` job to `.github/workflows/ci.yml`
- [ ] Measures line and branch coverage using `cargo-llvm-cov`
- [ ] Generates LCOV report and uploads to Codecov (or step summary)
- [ ] Sets coverage target (e.g. >= 80% line coverage for core math/parsing logic)
- [ ] Does not fail on external generated XDR structures

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b ci/code-coverage`

Key files:
- `.github/workflows/ci.yml`

Use `taiki-e/install-action@cargo-llvm-cov` and `cargo llvm-cov --lcov --output-path lcov.info` in CI.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Enforcing 100% mutation testing coverage
- Private coverage dashboard hosting
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

# ═══════════════════════════════════════════════════════════════════════════════
# ARCHITECTURE, ERROR HANDLING & REFACTORING (#143–152)
# ═══════════════════════════════════════════════════════════════════════════════

create_issue "refactor: extract reusable RPC retry and backoff helper" "refactor, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Extract duplicate HTTP retry and backoff logic from `src/rpc/client.rs`, `src/rpc/simulate.rs`, and `src/rpc/config.rs` into a unified generic `with_retry` helper function.

## Background
Currently retry loops and error checking are duplicated across multiple RPC query methods. Consolidating into a single generic helper simplifies maintenance, standardizes backoff formulas, and reduces code duplication.

## Acceptance criteria
- [ ] Creates generic helper `pub async fn with_retry<F, Fut, T, E>(max_retries: u32, base_delay: Duration, op: F) -> Result<T, E>`
- [ ] Refactors all RPC call sites to use `with_retry`
- [ ] Eliminates duplicate loop boilerplate across `simulate_transaction`, `get_ledger_entries`, and health check methods
- [ ] All existing unit and integration tests continue to pass without regression

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/rpc-retry-helper`

Key files:
- `src/rpc/client.rs`
- `src/rpc/mod.rs`
- `src/rpc/simulate.rs`
- `src/rpc/config.rs`

Implement in `src/rpc/client.rs` using `tokio::time::sleep`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- External retry crate dependency if simple loop suffices
- Dynamic distributed backoff
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: enhance error reporting with structured error context and causes" "refactor, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Enhance `AppError` enum using `thiserror` attributes with structured error fields, clear error causes, and actionable suggestions for user mistakes.

## Background
When an error occurs (such as failing to load a WASM file, parse an XDR envelope, or connect to RPC), developers need error messages that clearly state: (1) what failed, (2) the underlying reason, and (3) a suggested fix.

## Acceptance criteria
- [ ] Audits all `AppError` variants in `src/error.rs`
- [ ] Adds structured fields (e.g. `path: PathBuf`, `endpoint: String`, `expected: String`, `got: String`) instead of raw generic strings
- [ ] Implements `source()` chaining on `thiserror::Error` for underlying I/O and network errors
- [ ] Formats errors consistently: `error: <action failed>: <underlying cause>. Suggestion: <hint>`
- [ ] Tests verify error Display output format across all variants

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/structured-error-context`

Key files:
- `src/error.rs`
- `src/main.rs`
- `src/wasm/parser.rs`
- `src/rpc/client.rs`

Update enum variants with explicit fields and `#[error("...")]` templates.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Replacing thiserror with anyhow across library interfaces
- Panic hook customization
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "feat(logging): add structured logging with `tracing` and `tracing-subscriber`" "enhancement, refactor, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Integrate the `tracing` and `tracing-subscriber` crates for hierarchical structured instrumentation (spans, events, timing) across all operations.

## Background
Replacing ad-hoc `eprintln!` calls with `tracing::debug!`, `tracing::info!`, and `tracing::instrument` enables unified log level filtering (`RUST_LOG=info,soroban_cost_estimator=debug`) and easy integration into developer tooling.

## Acceptance criteria
- [ ] Adds `tracing` and `tracing-subscriber` dependencies
- [ ] Instruments key functions: `parse_wasm`, `simulate_transaction`, `fetch_network_config`, `diff_snapshots` with `#[tracing::instrument]`
- [ ] Initializes `tracing_subscriber::fmt` in `src/main.rs` respecting `RUST_LOG` and `--verbose`
- [ ] Logs to stderr without interfering with stdout JSON streams
- [ ] All unit and integration tests pass

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b feat/tracing-instrumentation`

Key files:
- `Cargo.toml`
- `src/main.rs`
- `src/lib.rs`
- `src/rpc/simulate.rs`
- `src/wasm/parser.rs`

Initialize subscriber with `.with_writer(std::io::stderr)`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- OpenTelemetry OTLP remote collector exporter
- JSON log formatting
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: audit and eliminate all warnings under `clippy::pedantic`" "refactor, code-quality, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Audit the entire codebase under `cargo clippy --workspace --all-targets -- -D clippy::pedantic` and resolve any remaining warnings or code smells.

## Background
Maintaining zero clippy warnings under strict pedantic mode ensures idiomatic Rust, optimal memory utilization, and clean API design.

## Acceptance criteria
- [ ] Runs `cargo clippy --workspace --all-targets -- -D warnings` cleanly
- [ ] Audits allowed lints in `Cargo.toml` to remove unnecessary suppressions where feasible
- [ ] Replaces inefficient clones or redundant allocations with borrowed references
- [ ] Maintains 100% test pass rate

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/clippy-pedantic-cleanup`

Key files:
- `Cargo.toml`
- `src/lib.rs`
- `src/main.rs`
- `src/wasm/parser.rs`
- `src/cache.rs`

Fix lint suggestions like `manual_let_else`, `needless_pass_by_value`, and redundant borrows.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Rewriting algorithms from scratch
- Breaking public API signatures
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: consolidate CLI argument definitions into dedicated submodules" "refactor, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Refactor `src/cli.rs` by splitting command argument structs into dedicated submodules (`cli/estimate.rs`, `cli/config.rs`, `cli/rpc.rs`, `cli/wasm.rs`).

## Background
`src/cli.rs` is growing large as new commands and options are added. Organizing CLI definitions into dedicated submodules improves code navigation and separation of concerns.

## Acceptance criteria
- [ ] Splits `src/cli.rs` into `src/cli/mod.rs`, `src/cli/estimate.rs`, `src/cli/config.rs`, `src/cli/rpc.rs`, `src/cli/wasm.rs`
- [ ] Re-exports all public types from `src/cli` so external callers and tests are unaffected
- [ ] No breaking changes to CLI command syntax or flags
- [ ] Builds and tests pass cleanly

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/modular-cli-definitions`

Key files:
- `src/cli.rs`
- `src/cli/mod.rs`
- `src/cli/estimate.rs`
- `src/cli/config.rs`

Move clap structs into separate files and re-export in `src/cli/mod.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Switching CLI framework away from clap
- Changing command hierarchies
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: extract report formatting into a trait-based formatter system" "refactor, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Introduce a `ReportFormatter` trait with implementations for `TableFormatter`, `JsonFormatter`, `CsvFormatter`, and `MarkdownFormatter`.

## Background
Currently, report formatting functions are scattered across `cost_report.rs` with separate functions for each format. A trait-based system provides a clean, extensible abstraction for adding new formats.

## Acceptance criteria
- [ ] Defines `pub trait ReportFormatter { fn format_estimate(&self, report: &CostReport) -> String; fn format_estimate_all(&self, reports: &[CostReport]) -> String; fn format_diff(&self, diff: &ConfigDiff) -> String; }`
- [ ] Implements trait for `TableFormatter`, `JsonFormatter`, `CsvFormatter`, `MarkdownFormatter` in `src/report/`
- [ ] Refactors `src/main.rs` to dispatch formatting via trait object
- [ ] Existing formatting output is 100% preserved

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/trait-report-formatter`

Key files:
- `src/report/mod.rs`
- `src/report/cost_report.rs`
- `src/report/formatters/`

Create `src/report/formatters/` directory and implement each formatter.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Dynamic plugin loading of third-party formatters at runtime
- HTML canvas rendering
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: add `#[must_use]` annotations to all pure calculation functions" "refactor, code-quality, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Audit pure functions in `src/report/fee_calc.rs`, `src/xdr_helper.rs`, and `src/config_snapshot/diff.rs` and annotate them with `#[must_use]`.

## Background
Pure functions that compute values (e.g. fee math, XDR conversion, diff calculations) have no side effects. Adding `#[must_use]` ensures that callers never accidentally discard return values.

## Acceptance criteria
- [ ] Adds `#[must_use]` to all pure calculation, conversion, and constructor functions
- [ ] Compiler emits warnings if any pure calculation result is discarded
- [ ] `cargo clippy` runs cleanly with no `must_use_candidate` warnings
- [ ] All tests pass

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/must-use-annotations`

Key files:
- `src/report/fee_calc.rs`
- `src/xdr_helper.rs`
- `src/config_snapshot/diff.rs`
- `src/config_snapshot/model.rs`

Add `#[must_use]` attributes across pure functions in math and parsing modules.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Annotating impure async I/O functions
- Modifying external crate types
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: standardize error message formatting across all modules" "refactor, ux, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Establish and apply consistent error message styling guidelines (`what failed: underlying reason [context]`) across all crate modules.

## Background
Inconsistent error phrasing (e.g. mix of capitalized/lowercase, trailing periods, punctuation) makes CLI output feel unpolished. Standardizing on Rust CLI error conventions creates a cohesive user experience.

## Acceptance criteria
- [ ] Audits all error strings across `src/`
- [ ] Standardizes format: lowercase first letter (unless proper noun like WASM/RPC), no trailing period, colon-separated cause
- [ ] Example: `failed to parse WASM binary: invalid magic bytes` (not `Failed to parse wasm.`)
- [ ] All tests pass and verify updated error strings

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/standardize-error-messages`

Key files:
- `src/error.rs`
- `src/wasm/parser.rs`
- `src/rpc/client.rs`
- `src/config_snapshot/store.rs`

Review all `thiserror` messages and inline error returns against Rust API Guidelines.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Internationalization / multi-language translation strings
- Custom terminal error themes
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: implement `Display` trait for all public domain types" "refactor, code-quality, complexity: trivial, Stellar Wave" "$(cat <<'EOF'
## Summary
Implement standard `std::fmt::Display` for `CostReport`, `FeeBreakdown`, `ConfigDiff`, `CachedEstimate`, and `NetworkConfig`.

## Background
Implementing `Display` provides an intuitive default string representation for domain types, simplifying logging, testing, and library usage.

## Acceptance criteria
- [ ] Implements `Display` for `CostReport`, `FeeBreakdown`, `ConfigDiff`, `CachedEstimate`, and `NetworkConfig`
- [ ] Emits clean, human-readable summaries when formatted with `{}`
- [ ] Maintains existing `Debug` implementations with `{:?}`
- [ ] Unit tests verify `Display` output for each type

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/display-trait-impls`

Key files:
- `src/report/cost_report.rs`
- `src/report/fee_calc.rs`
- `src/config_snapshot/model.rs`
- `src/config_snapshot/diff.rs`
- `src/cache.rs`

Add `impl std::fmt::Display for ...` blocks formatting key summary metrics.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Implementing custom parsing `FromStr` for all domain types
- Colorized Display outputs
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"
create_issue "refactor: extract shared snapshot diffing and network fetch logic" "refactor, complexity: medium, Stellar Wave" "$(cat <<'EOF'
## Summary
Extract the repeated workflow pattern ('fetch live config -> construct snapshot -> find latest local snapshot -> compute diff -> detect staleness') into a reusable pipeline in `src/config_snapshot/`.

## Background
`cmd_config_diff`, `watch`, and `auto_snapshot` each implement variations of the same 5-step network diffing flow. Consolidating into a single reusable service function eliminates duplication and prevents logic drift.

## Acceptance criteria
- [ ] Creates `pub async fn run_live_config_diff(client: &RpcClient, network: &str) -> Result<ConfigDiffResult, AppError>` in `src/config_snapshot/mod.rs`
- [ ] Refactors `cmd_config_diff` and `watch` loop to call this shared function
- [ ] Eliminates ~80 lines of duplicate orchestration code in `src/main.rs`
- [ ] All unit and integration tests pass without regression

## Implementation hints
Audience: contributor.

Create branch: `git checkout -b refactor/config-diff-pipeline`

Key files:
- `src/config_snapshot/mod.rs`
- `src/main.rs`
- `src/rpc/config.rs`

Encapsulate live fetch, snapshot comparison, and stale estimate detection in `src/config_snapshot/mod.rs`.

## Repo-specific notes
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope
- Pluggable network transport middleware
- Asynchronous actor framework
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit
1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you.
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

EOF
)"

echo "Batch creation complete! Total 125 issues processed."
