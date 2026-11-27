# Incident report: malicious pull request #669

**Status:** contained in the repository tree; git history and downstream clones
are pending.
**Reported:** 2026-11-05
**Affected repository:** `Stellar-Cost-Labs/soroban-cost-estimator`
**Affected refs:** `main` from 2026-10-29 onward (merge commit `37dceae`)
**Published crate:** unaffected — `0.1.0` was published 2026-08-03, before this
change, and no `src/`, `build.rs`, `Cargo.toml`, or CI file was modified.

## Summary

A pull request titled `test: cover config diff permutations` (PR #669) was
merged on 2026-10-29. Alongside a genuine, if large, addition to
`tests/config_diff_tests.rs`, the branch carried files that had nothing to do
with the stated change: an editor configuration that runs a command when the
folder is opened, and two files named as web fonts whose contents are
obfuscated JavaScript.

The PR's stated change was test-only. Nothing in the code that produces the
`estimate`, `config snapshot`, `config diff`, or `watch` output was touched.

Everything below is described from a read of the repository only. No maintainer
machine has been examined, and no evidence of execution has been found or ruled
out.

## Timeline

| When (UTC+1) | Event |
|---|---|
| 2026-10-23 09:51 | Commit `6612519` adds `.vscode/`, `public/fonts/`, a rewritten `.gitignore`, and a large `test_comfy/` build tree alongside test additions. |
| 2026-10-23 08:18 | A second PR from the same handle (#668, `test: cover watch SIGINT shutdown`) is opened and remains unmerged. |
| 2026-10-23 → 2026-10-28 | The branch is repeatedly updated by merging `main` into it (`afb1469`, `6d40fe9`, `a4278c6`, `c2854f8`), keeping it current without changing the payload. |
| 2026-10-29 10:54 | PR #669 is merged as `37dceae`. The payload becomes reachable from `main`. |
| 2026-11-05 | Detected during a repository security review. Payload identified, contents decoded, and removed from the working tree. |

## Root cause

Three conditions had to line up:

1. **No review gate for non-source files.** The repository's protection
   required one approving review and a green `build` check. Both were
   satisfiable while the payload hid in directories nobody treats as code:
   `.vscode/`, `public/fonts/`, and a committed `target/` tree.
2. **The web UI hid the payload.** GitHub renders a diff as
   `Binary files a/… and b/… differ` when either side contains a NUL byte or
   is detected as binary. The two payload files were given font extensions, and
   `.gitignore` was rewritten with embedded NUL bytes, so the diff could not be
   read at a glance. A reviewer skimming the web diff saw a test-file change
   plus a wall of "Binary files differ" lines.
3. **Dilution by volume.** The same commit added roughly 149 MB of Rust build
   output under `test_comfy/`, which pushed every interesting file far down the
   file list.

## What the payload does

`.vscode/tasks.json` defines a task labelled `eslint-check` that runs
`node ./public/fonts/fa-solid-300.ttf` with `runOptions.runOn: "folderOpen"`,
and hides its own output (`"reveal": "never"`, `"hide": true`,
`"close": true`). `.vscode/settings.json` sets `task.allowAutomaticTasks: true`,
which permits automatic task execution on folder open.

The file it launches, `public/fonts/fa-solid-300.ttf`, is 12,681 bytes of
obfuscated JavaScript — not a font. It is byte-identical to
`public/fonts/fa-regular.lff`. Both build strings at runtime from reversed
fragments and `\u`-escaped keys, and both reference `require`/`module`.
`public/fonts/README.md` refers to a "Blockchain Explorer application" and to a
`public/index.html` that has never existed in this repository.

The practical consequence: opening a clone of this repository in VS Code, in a
workspace where the folder had been trusted, would run that JavaScript with the
user's privileges the moment the folder was opened.

## Scope

Affected:

- `main` at `37dceae` and every commit after it, plus every fork and clone made
  since 2026-10-29.
- Anyone who opened such a clone in VS Code, or who ran `npm`-style tooling or
  an editor task that walked into `.vscode/`.

Not affected:

- The published crate `soroban-cost-estimator 0.1.0` on crates.io (published
  2026-08-03, before the change; no build inputs were modified).
- The Rust source tree: `src/`, `build.rs`, `Cargo.toml`, `Cargo.lock`,
  `deny.toml`, and `.github/workflows/` were untouched by the malicious commit.
- The `v0.1.0` tag and its commit.

## Removal

Removed from the tree and from the index:

- `.vscode/` (5 files)
- `public/` (18 files: the two payload files plus 15 unrelated Font Awesome
  assets and a README describing an application that does not exist here)
- `test_comfy/` (a nested crate with committed `target/` output, ~149 MB)
- The rewritten `.gitignore`, replaced with clean LF text that no longer ignores
  itself, `target2/`, `temp_auto_push.bat`, or `branch_structure.json`

Because the objects remain reachable from history, removal from the working
tree is not enough on its own — see "Pending" below.

## Indicators of compromise

Treat any of these as evidence a machine saw the payload:

- SHA-256 `7ba0a30eabfa46008d6417532533199bbf351b27f3841477e9133f43fb404d24`
  (both payload files are identical)
- Git blob `38ba99d7bb27fdeb45b82c6fdffe8ecca03108bb`
  (`public/fonts/fa-regular.lff`)
- Files named `fa-solid-300.ttf` or `fa-regular.lff` whose content is ASCII
  text rather than a font
- A `.gitignore` that `file` reports as `data` and `git diff` reports as
  "Binary files differ"
- A VS Code task labelled `eslint-check` that executes a font file
- Committed paths `test_comfy/`, `target2/`, `branch_structure.json`,
  `temp_auto_push.bat`

## Controls added

- `scripts/security-scan.sh` — structural scanner. Fails on text in a
  binary-extension file, NUL bytes in text/config files, committed ELF/PE
  binaries, editor auto-run hooks, download-and-execute markers, and any change
  under `.vscode/`, `.devcontainer/`, `.idea/`, `.cursor/`, `.claude/`,
  `.zed/`, `.cargo/`, `.githooks/`. Runs against full history with
  `--history`.
- `.github/workflows/security.yml` — runs the scanner **from the base branch**
  (`git show origin/${{ github.base_ref }}:scripts/security-scan.sh`) against
  the PR range, so a pull request cannot weaken the check that judges it. No
  `pull_request_target`. Actions pinned to commit SHAs. Write permissions
  removed. `cargo deny` and `cargo audit` run alongside.
- `.github/CODEOWNERS` — covers workflows, scripts, build inputs, dotfile
  directories, and the security policy.
- `.gitattributes` — forces `text=auto eol=lf` so text cannot silently become
  "binary" in review.
- `.github/pull_request_template.md` — mandatory supply-chain checklist, so
  every future PR states explicitly that it adds no binary, no dotfile, and no
  auto-run hook.
- `.github/dependabot.yml` — grouped weekly dependency and Actions updates, so
  pin drift is routine rather than an event.

## Pending

These remain open and are tracked in `docs/MAINTAINER-CHECKLIST.md`:

- Rotating credentials used on any machine that opened a clone since
  2026-10-29 (GitHub token, crates.io token, cloud keys, SSH keys).
- Purging the objects from history (`git filter-repo`), which rewrites SHAs.
- Asking GitHub Support to drop cached commit views and `refs/pull/669`.
- Tightening repository settings (ruleset, secret scanning, push protection,
  read-only default `GITHUB_TOKEN`).
- Notifying contributors and anyone who cloned since 2026-10-29.

## Note on attribution

This report describes files and their behaviour. It does not assert who wrote
them or why. The account that opened PR #669 may itself be a compromised or
impersonated account, and the same handle has other, apparently ordinary,
contributions in this repository. No accusation is made or implied.
