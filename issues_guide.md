# Creating Meaningful Issues for Soroban Cost Estimator

A practical guide for maintainers and contributors on writing high-quality issues
that lead to real progress, clear reviews, and a stronger community.

> Based on [Creating Meaningful Issues: A Guide for Maintainers](https://www.drips.network/blog/posts/creating-meaningful-issues) by Drips.

---

## Why Issue Quality Matters

Every issue is an opportunity. Clear, well-scoped issues help contributors do
their best work and keep the project moving forward. Low-effort issues lead to
low-quality contributions, frustrated developers, and short-term participation
instead of long-term building.

---

## Five Principles for Creating Meaningful Issues

### 1. Focus on Real Impact

Before publishing an issue, ask: **does this meaningfully improve the tool,
developer experience, or user outcomes?**

If the answer is "meh," rethink it. Good issues create momentum, not noise.

| ✅ Real impact | ❌ Low impact |
|---|---|
| Add `--json` output to `config diff` | Fix a trailing space in a log message |
| Improve fee-rate degradation warnings | Rename an internal variable |
| Support a new `ConfigSetting` entry | Add a comment to an existing function |

### 2. Provide Clear Context

Give contributors the **why**, not just the what. Share background, the problem,
and what "done" should look like.

Context helps contributors make better decisions without constantly asking for
clarification. Reference relevant files, docs, or network behaviour.

### 3. Define Scope Appropriately

Issues should be **completable within a single contribution cycle**.

- **Too big** → overwhelming, hard to review
- **Too small** → not worth the overhead

Find the balance. If an issue is large, break it into smaller related issues.
If it feels trivial, combine it with related work or remove it entirely.

### 4. Include Implementation Guidelines (Without Micromanaging)

Offer direction, not handcuffs. Point contributors toward:

- **Key files/modules** (e.g. `src/rpc/client.rs`, `src/report/fee_calc.rs`)
- **Edge cases** to watch for (e.g. negative refundable fees, zero-arg functions)
- **Constraints** such as no floats in fee math, no `unwrap()` outside tests
- **How to validate** the change (e.g. `cargo test --all`, manual testnet run)

Leave room for contributors to apply their own judgement, but set expectations
clearly.

### 5. Be Explicit About Expectations and Complexity

Don't make contributors guess. Be clear about:

- What **"done"** looks like
- How you'll **review** the work
- What must be included in the **PR** (tests, docs, screenshots)
- The **complexity level** (see below)

---

## Complexity Levels and Points

Tag every issue with a complexity level. This sets clear expectations for
contributors and keeps work fairly weighted.

| Level | Points | Description | Examples in this project |
|-------|--------|-------------|--------------------------|
| **Trivial** | 100 | Small, clearly bounded changes with obvious acceptance criteria | Fix a typo in an error message, update a README section, add a missing doc comment |
| **Medium** | 150 | Standard features or logic touching multiple parts of the codebase | Add `--json` output to a command, implement a new `ConfigSetting` parser, add progress indicator to `estimate-all` |
| **High** | 200 | Complex engineering work such as integrations or architectural changes | Add a new CLI command (e.g. `watch`), implement fee-rate source degradation warnings, add estimate caching with staleness detection |

Tag issues honestly. Don't inflate easy work or underprice hard tasks. When
contributors feel points are fair, you attract builders who care about the work,
not just the reward.

---

## Issue Template

Use this structure for every issue. It follows the format used in Drips Wave
issue boards.

```markdown
# <short imperative title>

## Summary

<one-sentence description of what this issue asks for.>

## Background

<why this matters. what problem does it solve? what is the current behaviour
and why is it insufficient? reference relevant files, docs, or user workflows.>

## Acceptance criteria

- <specific, testable criterion>
- <specific, testable criterion>
- <specific, testable criterion>
- Lint, type-check, and tests all pass locally.
- PR description references this issue with `Closes #`.

## Implementation hints

<audience: pick one (contributor / operator / downstream integrator) and write
for them. provide concrete guidance: key files, patterns to follow, edge cases
to watch for, constraints. prefer concrete examples over abstract definitions.>

## Repo-specific notes

<project-specific rules: coding standards, CI commands, validation steps.>

## Out of scope

- <what NOT to do in this issue>
- <adjacent work that should be a separate issue>
- <anything beyond the acceptance criteria — surface follow-ups as separate issues>

## How to claim and submit

1. Comment on this issue saying you'd like to take it on; wait for a maintainer
   to assign you (avoids duplicated effort).
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.
```

---

## Examples

### ✅ Good Issue

```markdown
# Add `--json` output to `config diff`

## Summary

Add machine-readable JSON output to the `config diff` command.

## Background

CI pipelines and automation tools need structured output from `config diff` to
detect pricing changes programmatically. Currently only `estimate` and
`config snapshot` support `--json`, so teams monitoring network pricing in CI
have no clean way to parse the diff output. The `config diff` command already
serializes its result internally (`ConfigDiff` derives `Serialize`) — the JSON
path just needs to be exposed.

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
```

### ❌ Bad Issue

```
Fix the config diff.
```

No context, no scope, no acceptance criteria, no complexity tag. A contributor
has no idea what "fix" means or where to start.

---

## PR Guidelines

When submitting a PR for an issue:

1. **Get assigned before starting** — avoids duplicate effort.
2. **Reference the issue**: Include `Closes #<issue_id>` in the PR description.
3. **One logical unit per PR** — keep changes focused and reviewable.
4. **Conventional commit format**: `type(scope): description`
   (e.g. `feat(rpc): add getLedgerEntries config fetch`).
5. **Ensure CI passes** before pushing:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

6. **Demonstrate the change**: show before/after output, test results, or
   reproduction steps as relevant to the issue.
7. **Keep it scoped** — don't bundle unrelated changes into the same PR.

---

## Recognition

After contributions are merged, maintainers may award **Compliments** for work
that genuinely exceeds expectations. Use these sparingly to highlight exceptional
contributions — not to rebalance points.

---

## Quick Checklist for Issue Authors

Before you hit "Create issue," verify:

- [ ] Does the **Summary** capture the goal in one sentence?
- [ ] Does the **Background** explain why this matters (not just what to build)?
- [ ] Are **acceptance criteria** specific, testable, and complete?
- [ ] Are **implementation hints** concrete (key files, patterns, examples)?
- [ ] Is there a **repo-specific notes** section with CI commands?
- [ ] Is there an **out of scope** note to prevent scope creep?
- [ ] Is the **complexity level** tagged honestly?
- [ ] Is the **"How to claim and submit"** section included?

---

*Create issues like your community's time matters. Because it does.* 🌊
