# Security Policy

## Disclaimer

This is **unaudited developer tooling**. It wraps Stellar's `simulateTransaction`
RPC to provide fee estimates and network config monitoring. **Do not rely solely
on this tool's output for financial decisions involving mainnet deployments.**
Always verify fee estimates against your target network before mainnet deploy.

## Reporting a Vulnerability

Please report privately, not in a public issue:

- **Preferred:** [GitHub private vulnerability reporting](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/security/advisories/new)
  (Security → Advisories → Report a vulnerability).
- **Alternative:** a direct message to the maintainer.

We will acknowledge receipt within 48 hours and work to address the issue
promptly. Please do not disclose the vulnerability publicly until we've had
a chance to address it.

Never include live credentials, tokens, or private keys in a report. Redact
them; we will ask if we need more.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Security Considerations

- This tool makes read-only RPC calls (`simulateTransaction`,
  `getLedgerEntries`). It does not deploy contracts or submit transactions.
- Config snapshots are stored locally in `~/.soroban-cost-estimator/`.
  Protect this directory if it contains sensitive network information.
- The tool does not use or manage secret keys. All simulations use a dummy
  source account (all zeros).

## Supply-chain controls

This repository enforces controls that apply to contributors as well as to the
maintainer. They exist because of a real incident — see
[docs/INCIDENT-REPORT.md](docs/INCIDENT-REPORT.md).

- **A pull request is data, not code.** Review it with `gh pr diff` or in
  github.dev. Do not check out a fork branch in an editor with workspace trust
  enabled.
- **No opaque files.** Text hidden behind a binary extension is rejected, as are
  NUL bytes inside text or config files.
- **No editor auto-run.** `.vscode/`, `.devcontainer/`, `.idea/`, `.cursor/`,
  `.claude/`, `.zed/`, `.cargo/`, and `.githooks/` are gated: a change there
  fails CI unless the maintainer has approved it with the
  `security-review-approved` label.
- **The gate cannot be edited by the change it judges.**
  `.github/workflows/security.yml` runs `scripts/security-scan.sh` **from the
  base branch**, never from the pull request.
- **Machine-readable enforcement.** `scripts/security-scan.sh --diff <range>`
  and `--history` exit non-zero on any blocking finding. Run them locally
  before opening a PR that touches anything unusual.
- **Dependencies are pinned in CI.** Workflow actions are referenced by commit
  SHA, `GITHUB_TOKEN` is read-only by default, and `cargo deny` plus
  `cargo audit` run on every pull request.
