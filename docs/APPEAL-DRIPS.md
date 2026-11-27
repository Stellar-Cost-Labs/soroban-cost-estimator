# Draft: message to Drips / Stellar Wave

> **Do not send yet.** Everything marked `PENDING` below must be completed
> first. The point of this draft is that it contains no claim that is not
> verifiable from the repository. Replace the `PENDING` markers with dates once
> done, and delete any line you cannot stand behind.

---

**Subject:** Security incident disclosure — `soroban-cost-estimator`
(Stellar-Cost-Labs)

Hello,

I am the maintainer of `soroban-cost-estimator`, an open-source Soroban cost
estimation CLI. I am writing to disclose a supply-chain incident in the
repository and to describe what has been fixed.

**What happened.** A pull request that presented itself as a test-only change
(PR #669, merged 2026-10-29) also added files outside the scope of its
description: a VS Code task configured to run on folder open, and two files
named as web fonts whose contents are obfuscated JavaScript. Opening a clone of
the repository in VS Code on a trusted workspace would have executed that file.
A rewritten `.gitignore` containing NUL bytes caused the diff to render as
"Binary files differ", which is why the payload was not visible in the web
review. The same commit added roughly 149 MB of committed build output, which
pushed the interesting files down the diff.

**What was not affected.** The published crate `soroban-cost-estimator 0.1.0`
(published 2026-08-03) predates the change, and the malicious commit did not
modify `src/`, `build.rs`, `Cargo.toml`, `Cargo.lock`, or any CI workflow. The
`v0.1.0` tag and its commit are unchanged. The command-line behaviour users
depend on is intact.

**What has been done.**

- Payload removed from the repository tree (`.vscode/`, `public/`,
  `test_comfy/`), and the poisoned `.gitignore` replaced with clean text.
- A structural security scanner (`scripts/security-scan.sh`) that fails on text
  hidden behind a binary extension, NUL bytes in text files, committed
  binaries, editor auto-run hooks, and download-and-execute patterns; it runs
  over full history as well as per-PR.
- A pull-request security gate that runs that scanner **from the base branch**
  (`git show origin/${{ github.base_ref }}:scripts/security-scan.sh`), so a PR
  cannot weaken the check that judges it. No `pull_request_target`; actions
  pinned to commit SHAs; `contents: read` only; `cargo deny` and `cargo audit`
  in the same workflow.
- `CODEOWNERS` covering workflows, scripts, build inputs, and editor
  directories.
- A pull-request template with a mandatory supply-chain checklist, and
  Dependabot for grouped dependency and Actions updates.
- A public incident report at `docs/INCIDENT-REPORT.md`.

**Pending (in progress at the time of writing).**

- Credential rotation on affected machines — `PENDING`
- History purge (`git filter-repo`) so the objects are unreachable — `PENDING`
- GitHub Support request to purge cached commit views and `refs/pull/669` —
  `PENDING`
- Repository hardening (ruleset, secret scanning, push protection, read-only
  default `GITHUB_TOKEN`) — `PENDING`
- Contributor and downstream notice — `PENDING`

I am reporting this proactively rather than after an external report, and I am
happy to provide whatever detail is useful. I want to be explicit that the
account that opened the PR may itself be a victim of a compromised or
impersonated credential; the repository had no review gate for non-source files
at the time, and that is the failure I own and have fixed.

Thank you for your time.

Maintainer, `soroban-cost-estimator`
