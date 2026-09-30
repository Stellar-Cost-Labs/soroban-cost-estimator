# Sample Issues

Three realistic issues for the Soroban Cost Estimator, written using the
[issue template](issues_guide.md) format.

---

# Add `--json` output to `config diff`

## Summary

Add machine-readable JSON output to the `config diff` command.

## Background

The `estimate` and `config snapshot` commands both support `--json` for
machine-readable output, but `config diff` does not. CI pipelines and automation
tools that want to detect network pricing changes programmatically have no clean
way to parse the diff — they either depend on fragile stdout parsing or skip the
tool entirely.

The `ConfigDiff` struct in `src/config_snapshot/diff.rs` already contains all the
data needed for a structured response: field-level changes with old/new values,
pricing-change flags, and snapshot metadata. The JSON path just needs to be
exposed through the CLI. This is the last `config` subcommand without `--json`
support.

## Acceptance criteria

- `config diff --json` prints a structured JSON object to stdout containing:
  `old_snapshot`, `new_snapshot`, `changes` (array of `field_path`, `old_value`,
  `new_value`, `is_pricing_change`), and `has_pricing_changes`.
- Exit code behaviour is unchanged: 0 = no changes, 1 = pricing changes detected.
- Non-JSON output is unaffected when `--json` is not passed.
- Stale-estimate data is included in JSON output as a `stale_estimates` array.
- Lint, type-check, and tests all pass locally.
- PR description references this issue with `Closes #`.

## Implementation hints

Audience: contributor.

Create a branch: `git checkout -b feat/config-diff-json`

Key files:
- `src/cli.rs` — add `#[arg(long)] json: bool` to `ConfigAction::Diff` (copy
  the pattern from `ConfigAction::Snapshot`).
- `src/main.rs` — `cmd_config_diff` needs to accept `json_flag: bool` and
  conditionally serialize the `ConfigDiff` struct.
- `src/config_snapshot/diff.rs` — no changes needed; `ConfigDiff` already
  derives `Serialize`.

For the stale-estimate data in JSON mode, use a wrapper struct:
`{ "diff": ConfigDiff, "stale_estimates": Vec<CachedEstimate> }`.

## Repo-specific notes

Run these before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope

- Adding `--json` to the `watch` command (streaming output, different design).
- Changing exit code semantics.
- Adding filters (e.g. `--pricing-only`).
- Anything beyond the acceptance criteria above; surface follow-ups as separate
  issues.

## How to claim and submit

1. Comment on this issue saying you'd like to take it on; wait for a maintainer
   to assign you (avoids duplicated effort).
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

---

# Add `--rpc-url` to `estimate-all`

## Summary

Add `--rpc-url` override to the `estimate-all` command.

## Background

The `estimate` command accepts `--rpc-url` to override the default RPC endpoint
for a given network — useful for local `stellar/quickstart` nodes, custom
testnets, or private mainnet nodes behind a proxy. The `estimate-all` command
does not have this flag, so it always calls `resolve_endpoint(network, None)`,
hardcoding the default endpoint.

This means `estimate-all` cannot be run against a local development node, even
though `estimate` can. For teams running `estimate-all` in CI against a local
`stellar/quickstart` container, this is a hard blocker — they have to fall back
to calling `estimate` manually for each function.

## Acceptance criteria

- `estimate-all` accepts `--rpc-url <url>` as an optional argument, identical
  to the one on `estimate`.
- When provided, `estimate-all` uses the custom endpoint instead of the default.
- When not provided, behaviour is unchanged.
- The flag is documented in the CLI help text.
- Lint, type-check, and tests all pass locally.
- PR description references this issue with `Closes #`.

## Implementation hints

Audience: contributor.

Create a branch: `git checkout -b feat/estimate-all-rpc-url`

Key files:
- `src/cli.rs` — add `#[arg(long)] rpc_url: Option<String>` to the
  `EstimateAll` variant (copy the pattern from `Estimate`).
- `src/main.rs` — `cmd_estimate_all` needs to accept `rpc_url: Option<&str>`
  and pass it to `resolve_endpoint(network, rpc_url)`. The call on line ~184
  becomes `resolve_endpoint(network, rpc_url)`.

The `estimate_all_function` helper doesn't need changes — it receives an
already-constructed `RpcClient`.

## Repo-specific notes

Run these before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope

- Adding `--rpc-url` to `config snapshot`, `config diff`, or `watch` (separate
  issues).
- Adding network presets/aliases.
- Anything beyond the acceptance criteria above; surface follow-ups as separate
  issues.

## How to claim and submit

1. Comment on this issue saying you'd like to take it on; wait for a maintainer
   to assign you (avoids duplicated effort).
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.

---

# Add `--clear-cache` flag to wipe the estimate cache

## Summary

Add a `--clear-cache` flag and `config cache clear` subcommand to wipe the
estimate cache.

## Background

The estimate cache (`~/.soroban-cost-estimator/cache/`) stores past simulation
results keyed by wasm hash, function name, and args hash. The `config diff`
command cross-references this cache to report stale estimates. But there is no
way to clear the cache from the CLI — users must manually navigate to the
directory and delete JSON files.

After upgrading the tool, after a major network upgrade, or after debugging a
bad estimation, this is tedious and error-prone. Most users won't know where the
cache lives, and the ones who do shouldn't have to `rm` files by hand.

## Acceptance criteria

- `estimate --clear-cache` clears cached estimates for the current network
  before running the estimate, printing `"Cleared N cached estimate(s) for
  <network>."`.
- `config cache clear` clears the cache for the default network (testnet).
- `config cache clear --network mainnet` clears only mainnet entries.
- Both paths use the same underlying function so behaviour is identical.
- The `--clear-cache` flag can be combined with other `estimate` flags — it
  runs the clear before the simulation.
- Clearing one network does not affect another network's cache entries.
- Lint, type-check, and tests all pass locally.
- PR description references this issue with `Closes #`.

## Implementation hints

Audience: contributor.

Create a branch: `git checkout -b feat/clear-cache`

Key files:
- `src/cache.rs` — add `clear_cache(network: &str) -> AppResult<usize>` that
  deletes `.json` files where the entry's `network` field matches, returning
  the count of deleted files.
- `src/cli.rs` — add `#[arg(long)] clear_cache: bool` to `Estimate`; add a
  `Cache { Clear { network } }` subcommand under `ConfigAction`.
- `src/main.rs` — wire the flag and subcommand to the new `clear_cache`
  function.

The `CachedEstimate` struct already has a `network` field, so filtering is
straightforward: read each file, deserialize, check the network, delete if it
matches.

## Repo-specific notes

Run these before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope

- Adding a `--max-age` flag to clear only old entries.
- Adding a `config cache list` subcommand (separate issue).
- Auto-clearing stale entries during `estimate`.
- Anything beyond the acceptance criteria above; surface follow-ups as separate
  issues.

## How to claim and submit

1. Comment on this issue saying you'd like to take it on; wait for a maintainer
   to assign you (avoids duplicated effort).
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.
