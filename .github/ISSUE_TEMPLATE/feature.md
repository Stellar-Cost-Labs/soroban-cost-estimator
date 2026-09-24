---
name: Feature Request
about: Propose a new feature or improvement for soroban-cost-estimator
title: ""
labels: enhancement
---

## Summary

<!-- One-sentence description of what this issue asks for. -->

## Background

<!-- Why this matters. What problem does it solve? What is the current behaviour
and why is it insufficient? Reference relevant files, docs, or user workflows. -->

## Acceptance criteria

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3
- [ ] Lint, type-check, and tests all pass locally.
- [ ] PR description references this issue with `Closes #`.

## Implementation hints

<!-- Audience: pick one (contributor / operator / downstream integrator) and
write for them. Provide concrete guidance: key files, patterns to follow, edge
cases to watch for, constraints. Prefer concrete examples over abstract
definitions. -->

## Repo-specific notes

<!-- Project-specific rules: coding standards, CI commands, validation steps. -->

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Out of scope

<!-- What NOT to do in this issue. Adjacent work that should be a separate
issue. Anything beyond the acceptance criteria — surface follow-ups as separate
issues. -->

## How to claim and submit

1. Comment on this issue saying you'd like to take it on; wait for a maintainer to assign you (avoids duplicated effort).
2. Open a PR that references this issue (`Closes #`).
3. Make sure CI is green and request review from a maintainer.
