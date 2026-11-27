# Maintainer checklist — actions that require repository admin access

Every item below needs the GitHub UI, a credential only the maintainer holds, or
a decision only the maintainer can make. Work top to bottom: the history rewrite
(item 4) invalidates SHAs, so do it before anything that points at a commit.

Legend: **PENDING** = not done yet. Tick as you go.

## 0. Before anything else

- [ ] **PENDING** — Decide whether any machine opened a clone of this repo
      between 2026-10-29 and now in VS Code. If yes, treat that machine as
      compromised (item 3) and do not defer it.
- [ ] **PENDING** — Do the rest of this work from a machine that never opened a
      clone of this repository.

## 1. GitHub settings (do these first — they are cheap and reversible)

Ruleset on `main` (Settings → Rules → Rulesets → New branch ruleset):

- [ ] **PENDING** — Target `main`. Enforcement **Active**. **No bypass list** —
      not even for your own account.
- [ ] **PENDING** — Require a pull request before merging; required approvals
      `1`; dismiss stale approvals; require approval of the most recent push.
- [ ] **PENDING** — Require status checks to pass: `build`, `structural scan`,
      `supply chain (deny + audit)`. Require branches to be up to date.
- [ ] **PENDING** — Require signed commits. Block force pushes. Block deletions.
- [ ] **PENDING** — Require linear history (optional but recommended).

Repository settings:

- [ ] **PENDING** — Settings → Code security: enable **secret scanning** and
      **push protection**; enable **Dependabot alerts** and **security updates**.
- [ ] **PENDING** — Settings → Actions → General: **Fork pull request
      workflows** → "Require approval for all outside collaborators";
      **Workflow permissions** → "Read repository contents"; disable "Allow
      GitHub Actions to create and approve pull requests".
- [ ] **PENDING** — Settings → Actions → General → **Access**: keep only
      allowlisted actions, or at minimum review the list.
- [ ] **PENDING** — Settings → Code security → **Code scanning**: enable
      CodeQL default setup for Rust-adjacent files and Actions workflows.
- [ ] **PENDING** — Settings → Branches → confirm nothing bypasses the new
      ruleset (`git push --force` on `main` must fail).

## 2. Audit the GitHub side (read-only, but admin-scoped)

- [ ] **PENDING** — Security log (Settings → Security log): filter
      2026-10-23 → today. Look for: new PATs, OAuth app authorisations, SSH/GPG
      key additions, org membership changes, `repo.transfer`, ruleset edits.
- [ ] **PENDING** — Settings → Deploy keys: any key present that you did not
      add. Check "Allow write access" on each.
- [ ] **PENDING** — Settings → Webhooks: any endpoint you do not recognise,
      especially ones pointing at a personal domain, `ngrok`, or a paste site.
- [ ] **PENDING** — Settings → Secrets and variables → Actions: list names and
      `Updated` timestamps. Any secret added or rotated since 2026-10-23 that
      you did not do is a compromise indicator. Rotate `CARGO_REGISTRY_TOKEN`,
      `CRATES_IO_TOKEN`, and `RELEASE_TOKEN` regardless.
- [ ] **PENDING** — Actions → Workflow runs since 2026-10-23: look for runs
      triggered by `pull_request_target`, runs on branches you did not push,
      and any run whose logs show an unexpected `curl`, `node`, or `base64`.
- [ ] **PENDING** — Settings → Collaborators and teams: remove anyone who does
      not need write access. Confirm no outside collaborator has admin.
- [ ] **PENDING** — Settings → GitHub Apps and Settings → Applications
      (OAuth): revoke anything you cannot justify. Check the **Drips Wave** app
      scope while you are here.
- [ ] **PENDING** — Review open PRs, especially #668 (same handle as #669).
      Review with `gh pr diff 668` or in github.dev. Do not check the branch out.
- [ ] **PENDING** — Decide the fate of PR #669's test additions: keep the
      `tests/config_diff_tests.rs` hunks (they are legitimate and pass) and
      discard everything else.

## 3. Credential rotation

- [ ] **PENDING** — Rotate the GitHub PAT / OAuth token used on any machine that
      opened a clone (`gh auth logout` then re-auth).
- [ ] **PENDING** — Rotate the crates.io API token (crates.io → Account →
      API Tokens → revoke, create, update `CARGO_REGISTRY_TOKEN`).
- [ ] **PENDING** — Rotate SSH keys and any GPG signing key whose passphrase was
      cached on an affected machine.
- [ ] **PENDING** — Rotate cloud/provider credentials reachable from that
      machine (AWS, GCP, Vercel, npm, Drip tokens).
- [ ] **PENDING** — Review `git config --global --list` and `~/.gitconfig`
      includes on affected machines for unexpected `core.hooksPath`,
      `url.*.insteadOf`, or credential helpers.

## 4. History rewrite (only after items 1–3)

Do this in a **fresh clone**, never in the repo you work in.

- [ ] **PENDING** — Mirror backup:
      `git clone --mirror <url> backup.git && tar -czf backup-<date>.tar.gz backup.git`
      Store it offline. Do not open it in an editor.
- [ ] **PENDING** — Fresh working clone: `git clone <url> rewrite && cd rewrite`.
- [ ] **PENDING** — Confirm provenance:
      `git log --all --name-only -- .vscode` must show only `6612519`.
- [ ] **PENDING** — `git filter-repo --strip-blobs-with-ids <ids> --path .vscode --path public/fonts --path test_comfy --path target2 --invert-paths`
- [ ] **PENDING** — Re-apply the cleaned tree as three logical commits:
      (1) remove payload and poisoned config, (2) scanner and CI gate,
      (3) repository cleanup and docs.
- [ ] **PENDING** — Verify `git diff --stat <rewritten HEAD> <local HEAD>` is
      empty, `git fsck` is clean, `v0.1.0` still points at `5752862`, and
      `git rev-list --objects --all | grep 38ba99d7` is empty.
- [ ] **PENDING** — Run `scripts/security-scan.sh --history` on the rewritten
      tree and require exit 0.
- [ ] **PENDING** — Re-run the sandboxed offline build and tests on the
      rewritten tree.
- [ ] **PENDING** — Relax branch protection, push `--force-with-lease` for
      `main` and tags only, using a fresh repo-scoped short-lived token, then
      re-enable the ruleset immediately.

## 5. Tell people

- [ ] **PENDING** — File the GitHub Support request: report PR #669, ask for
      cached commit views and `refs/pull/669` to be purged. Use the draft in
      `docs/INCIDENT-REPORT.md` as the body.
- [ ] **PENDING** — Post the contributor notice: re-clone, do not open an old
      clone in VS Code, rotate credentials if you did.
- [ ] **PENDING** — Publish `docs/INCIDENT-REPORT.md`.

## 6. Appeal and programme paperwork

- [ ] **PENDING** — Re-verify the current Drips Wave contributor and maintainer
      requirements page before submitting; the requirements were not verifiable
      from this environment. Complete every `PENDING` marker in
      `docs/APPEAL-DRIPS.md` before sending.
- [ ] **PENDING** — Confirm `cargo info soroban-cost-estimator` reports `0.1.0`
      from a maintainer machine.
