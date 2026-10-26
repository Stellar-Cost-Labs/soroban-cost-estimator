# Add JSON output to `config diff` command

## Summary

Add machine-readable JSON output to the `config diff` command. CI pipelines and automation tools need structured output from `config diff` to detect pricing changes programmatically. Currently only `estimate` and `config snapshot` support `--json`, so teams monitoring network pricing in CI have no clean way to parse the diff output. The `config diff` command already serializes its result internally (`ConfigDiff` derives `Serialize`) — the JSON path just needs to be exposed.

## Background

CI pipelines and automation tools need structured output from `config diff` to detect pricing changes programmatically. Currently only `estimate` and `config snapshot` support `--json`, so teams monitoring network pricing in CI have no clean way to parse the diff output. The `config diff` command already serializes its result internally (`ConfigDiff` derives `Serialize`) — the JSON path just needs to be exposed.

## Acceptance criteria

- `config diff --json` prints a structured JSON object to stdout containing: `old_snapshot`, `new_snapshot`, `changes` (array of `field_path`, `old_value`, `new_value`, `is_pricing_change`), and `has_pricing_changes`.
- Exit code behaviour is unchanged: 0 = no changes, 1 = pricing changes detected.
- Non-JSON output is unaffected when `--json` is not passed.
- Stale-estimate data is included in JSON output as a `stale_estimates` array.
- Lint, type-check, and tests all pass locally.
- PR description references this issue with `Closes #`.

## Implementation hints

Audience: contributor.

Create a branch: `git checkout -b feat/config-diff-json`

Key files:
- `src/cli.rs` — add `#[arg(long)] json: bool` to `ConfigAction::Diff` (copy the pattern from `ConfigAction::Snapshot`).
- `src/main.rs` — `cmd_config_diff` needs to accept `json_flag: bool` and conditionally serialize the `ConfigDiff` struct.
- `src/config_snapshot/diff.rs` — no changes needed; `ConfigDiff` already derives `Serialize`.

For the stale-estimate data in JSON mode, use a wrapper struct: `{ "diff": ConfigDiff, "stale_estimates": Vec<CachedEstimate> }`.

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
- Anything beyond the acceptance criteria above; surface follow-ups as separate issues.

## How to claim and submit

1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you (avoids duplicated effort).
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.