## What changed and why

<!-- The problem, the approach, and any trade-off you made. -->

Closes #

## How it was verified

<!-- The exact commands you ran and their results. If a check could not run,
     say which and why. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`

## Security checklist

Every box must be ticked. Untick nothing to "fix later" — say why instead.

- [ ] **No binary or opaque file is added.** I did not add `.ttf`, `.woff`,
      `.eot`, `.png`, `.zip`, `.so`, `.dll`, `.exe`, or any file whose content
      is not what its extension says.
- [ ] **No dotfile or editor config is added or changed** (`.vscode/`,
      `.devcontainer/`, `.idea/`, `.cursor/`, `.claude/`, `.zed/`, `.cargo/`,
      `.githooks/`, `.envrc`, `.mcp.json`). If one is genuinely required, the
      maintainer asked for it in the issue.
- [ ] **No automatic-execution hook.** Nothing I added runs on folder open,
      on save, or as a background task (`runOn: folderOpen`,
      `task.allowAutomaticTasks`, npm/pip `postinstall`, git hooks).
- [ ] **No download-and-execute.** No `curl | sh`, `wget | sh`, `eval(`,
      `new Function(`, `atob(`/`base64 -d` decoding, or `child_process`.
- [ ] **Dependency changes are explainable.** If `Cargo.toml`/`Cargo.lock`
      changed, I can name each crate, say why it is needed, and confirm it is
      the real crate (correct repository link, plausible download counts) —
      not a lookalike or typosquat.
- [ ] **No secrets, tokens, private RPC URLs, or personal paths** are in the
      diff, the commit messages, or the PR description.
- [ ] **`.gitignore`/`.gitattributes` untouched**, or the change is explained
      in the description. A change that hides files from review is a red flag.
- [ ] I understand that files in this PR may be executed by a maintainer's
      tooling, and I have read the diff myself line by line.

## Follow-ups

<!-- Anything intentionally left out of scope. -->
