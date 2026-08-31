# soroban-cost-estimator — Comprehensive Project Review

**Repo:** `aigbagbobila/soroban-cost-estimator` (https://github.com/aigbagbobila/soroban-cost-estimator)
**Review date:** 2026-08-05
**Scope:** Full review — what the project is, what has been built, current state (freshly re-verified today), code/quality assessment, and **precisely what remains**.
**Method:** All quality gates re-run locally on 2026-08-05; live repo state pulled via `gh` and `git`; prior Drips verification audit (2026-08-03) condensed into the Appendix.
<!-- review -->
---

## Executive summary

`soroban-cost-estimator` is a **complete, working, published CLI tool** for estimating Soroban (Stellar) contract resource costs, with a genuinely differentiated feature: it tracks the *network's own pricing configuration* over time and tells users when a cached estimate is stale because the network changed its prices.

**Overall verdict: ✅ healthy and submission-ready.** The MVP is 100% built and verified end-to-end against a real deployed testnet contract (CPU instructions exact match vs the native Stellar CLI; fee ≤0.011% divergence). All quality gates pass: **53/53 tests, clippy clean (pedantic=deny), fmt clean, CI green (6/6 recent runs on `main`)**. The crate is published on crates.io (`0.1.0`). Six scoped contributor issues exist and are genuinely unimplemented. The remaining work is **mostly user actions** (Drips Wave application) **plus a P1 contributor backlog** of six issues, and a handful of small repo-hygiene fix-ups found in this review.

---

## 1. What the project is

A Rust CLI that:

1. Reads a compiled Soroban `.wasm` (validating it, enumerating exported functions, decoding the `contractspecv0` section for typed params).
2. Builds a `TransactionEnvelope` with an `InvokeHostFunctionOp` (WASM upload, or invocation of a deployed contract via `--id --fn --arg`) and calls `simulateTransaction` on testnet/mainnet/futurenet RPC.
3. Produces a fee breakdown (non-refundable vs refundable) in integer stroops, derived **independently from the network's own `ConfigSetting*` rates** — no floating point, no hardcoded rates.
4. Snapshots all 6 `ConfigSetting*` ledger entries (XDR-decoded) as versioned JSON; `config diff` compares them field-by-field and cross-references the estimate cache to flag now-stale estimates.
5. `watch` polls config on an interval (graceful SIGINT/SIGTERM shutdown) for CI/cron monitoring.

**Differentiator (verified against the real competitor):** the [Stellar Resource Usage Report](https://github.com/57blocks/stellar-resource-usage-report) is a real-time JS/TS test-code profiler against a local `stellar/quickstart` container — it answers "what did my contract consume just now?" This tool works from compiled artifacts + live RPC and is the only one in the space that tracks **network pricing-config drift** as a first-class, versioned artifact.

---

## 2. What has been built — feature inventory

| Feature | Status | Notes |
|---------|--------|-------|
| WASM parsing + function enumeration | ✅ | `wasmparser`, exports + param counts |
| `estimate` (single invocation) | ✅ | Upload or `--fn --arg` against deployed contract |
| `estimate-all` (multi-function) | ✅ | Typed params from `contractspecv0`; `[i/N]` progress |
| `config snapshot` (6 config settings) | ✅ | Batched `getLedgerEntries`, XDR decode, versioned JSON |
| `config diff` + stale-cache detection | ✅ | Field-level diff; exit 1 on pricing change; names stale estimates |
| `watch` (polling) | ✅ | `s`/`m`/`h`/`d` intervals; graceful shutdown |
| `--json` output | ✅ | Machine-readable parity |
| Fee breakdown (non-refundable/refundable) | ✅ | Integer math, config-sourced rates, `.max(0)` floor with named edge case |
| Estimate result caching | ✅ | Keyed by wasm-hash + function + args-hash |
| Footprint read/write entries & bytes (real, not zeros) | ✅ | Decoded from simulation footprint |
| Fee-rate source degradation warnings | ✅ | Warns + zeroes only the affected rate |
| Verified against live testnet | ✅ | Cross-checked vs `stellar contract invoke --cost` |
| GitBook documentation site | ✅ | New since last audit — 15 pages, linked from README |

---

## 3. Code & architecture review

**Layout (~3,740 LOC source + ~580 LOC tests):**

```
src/
├── main.rs, cli.rs, lib.rs, error.rs, xdr_helper.rs, cache.rs
├── wasm/parser.rs            # validation, function enumeration, contractspecv0 decode
├── rpc/client.rs             # endpoint resolution + generic JSON-RPC call
├── rpc/simulate.rs           # simulateTransaction wrapper + flexible parsing (9 tests)
├── rpc/config.rs             # 6 ConfigSetting entries, batched fetch (2 tests)
├── report/fee_calc.rs        # fee math — integer stroops (6 tests)
├── report/cost_report.rs     # table + pretty-JSON formatting
└── config_snapshot/{model,store,diff}.rs
```

**Strengths:**
- **Fee math is exemplary** (`fee_calc.rs`): `checked_mul`/`saturating_add` everywhere, no floating point, the refundable `.max(0)` floor is documented with the exact legit edge case (fee-less response), and regression tests carry the *live cross-check numbers* (15,427-stroop case → 4,496 non-refundable). The original negative-refundable bug was root-caused and fixed, not clamped.
- **No `unwrap()`/`expect()` in production code** — the one violation found by the 2026-08-03 audit (`xdr_helper.rs:270`) was fixed in `38d6177` and re-verified. All remaining panics are inside `#[cfg(test)]`.
- **Error handling** is consistent (`AppError` enum + `AppResult<T>`, thiserror).
- **Dependencies are pinned and modest** — clap, tokio, reqwest, serde, wasmparser, stellar-xdr 27.0.0, comfy-table. No risky/unmaintained crates.
- **CI** (`ci.yml`, job `build`) runs fmt → clippy → build → fixture gen → tests; branch protection requires the `build` check + 1 review.

**Minor observations (non-blocking):**
- `lib.rs` has a crate-level `#![allow(dead_code)]` — it predates full wiring; today it can mask genuinely dead code. Worth revisiting once the P1 backlog lands (remove or narrow it and let clippy find the dead spots).
- `fee_calc.rs` saturates overflow to `i64::MAX` (`checked_mul(...).unwrap_or(i64::MAX)`) rather than erroring — acceptable for a cost estimator (huge inputs would be absurd), but a future cleanup could return an error instead.
- `cache.rs`'s `list_cached_estimates` silently swallows corrupt cache files (`if let Ok(...)`) — reasonable for a scan, but means a corrupted cache is invisible. A `cache stats` command (issue #5) would surface this.
- **Tests are all green but coverage is uneven**: fee math, parsing, cache, diff, and CLI are well covered; there is no test for the `watch` loop or signal handling (see issue #1), no fixture-level footprint assertion (issue #2), and `estimate-all`/`estimate` happy paths rely on manual live verification.

---

## 4. Quality gates — fresh re-verification (2026-08-05)

All re-run today on `main` @ `522b5f6`:

| Gate | Result (2026-08-05) |
|------|---------------------|
| `cargo test --all` | **53 passed, 0 failed** (24 lib · 1 bin · 11 cli · 7 cache · 6 config_diff · 4 parser · 0 doc) |
| `cargo clippy --all-targets --all-features` | **Clean, exit 0** (`all` + `pedantic` = `deny` in Cargo.toml) |
| `cargo fmt --check` | **Clean, exit 0** |
| CI on `main` (`gh run list`) | **6/6 most recent runs `completed/success`** (latest `30905452157`, 2026-08-04) |
| Local vs remote | `main` 0 ahead / 0 behind `origin/main` — everything pushed |
| Git history | **47 commits** on `main` (up from 31 at the 2026-08-03 audit) — real incremental history |
| Open issues (`gh issue list`) | **6 open**, all `Stellar Wave` label, bodies intact |
| Published crate | `0.1.0` published to crates.io per the 2026-08-03 audit (direct re-fetch today returned HTTP 403 — crates.io API data-access policy; re-confirm with `cargo info soroban-cost-estimator` locally) |
| Live testnet cross-check | CPU 524,389 **exact**; fee ≤0.011% vs native CLI; footprint 1/1 entries, 136 write bytes, 156 tx bytes — reproduced on a later ledger (2026-08-04) within the documented ~20% drift margin |

---

## 5. Documentation review

- **README** — accurate and audited line-by-line (2026-08-03): differentiator grounded in mechanism, endpoints match `rpc/client.rs`, 6 config settings match code, install instructions verified, live-testnet table matches. All 4 badges return 200. ✅
- **GitBook docs site** — NEW since the last audit: `docs/` (15 pages: introduction, installation, concepts ×3, commands ×5, architecture with Mermaid, verification, contributing, FAQ) committed on `main`, linked from the README badge + banner. Structure is complete and internally consistent. ✅
- **Fixture record** (`tests/fixtures/contract/README.md`) — canonical cross-check record with contract ID, both number sets, and reproduction steps. ✅

---

## 6. Repository hygiene — findings from this review

These are new observations from the 2026-08-05 pass (not covered by the prior audit):

1. **Uncommitted working tree** — `review.md` (this file) is *untracked*, and `LICENSE-APACHE` has a whitespace-only uncommitted edit (two blank lines added before the copyright line). **Commit or discard both.**
2. **Stale local branches** — `docs-site` (16 ahead / 15 behind `main`) and `backup/docs-site` contain an **abandoned mdBook + GitHub Pages variant** of the docs, superseded by the GitBook approach merged onto `main`. They're local-only (not on origin) and nothing references them. **Delete them** (`git branch -D docs-site backup/docs-site`) once you're certain the GitBook route is final.
3. **ROADMAP.md is stale on two points** — it still says "real publish pending a crates.io token" (the crate was in fact published 2026-08-03 per the audit's captured crates.io response) and "no docs site built" (a GitBook site now exists and is linked from the README). Needs a Session-7 refresh to remain the "single source of truth."
4. **Empty GitHub repo description** — `gh repo view` shows `description: ""`. Set it to match the crate description.
5. **No `--version` flag** — the CLI help shows commands but no version. One-liner in clap (`#[command(version)]`); the README never claims it, so this is polish, not a defect.
6. **Governance note** — `main` history contains `8e866a2 chore: verify ruleset bypass` and its revert `1869a4f`. The bypass was reverted, and branch protection is confirmed live, but the pattern itself shouldn't recur; if a ruleset temporarily blocks legitimate docs work, use a PR + temporary rule, not a bypass commit on `main`.

---

## 7. What is left to be done

### 7.1 P0 — user actions (blocking the submission, minutes each)

1. **Apply to Drips Wave (browser).** Sign in with GitHub → install the **Drips Wave GitHub App** on the org → sync and apply this repo to the **Stellar Wave Program**. Immediately before applying, re-confirm (fresh): the repo isn't already on the approved list, and the 57blocks competitor scope is still as verified (real-time test instrumentation; no config-drift tracking — this tool's differentiator stands). If rejected: appeal after ≥2 weeks with substantive repo changes (max 3 appeals).
2. **crates.io publish — confirm/complete.** The 2026-08-03 audit captured a live crates.io response showing `0.1.0` published (`published_by aigbagbobila`, `audit_actions: publish 2026-08-03T07:56:32Z`), so the publish appears **done**. Because crates.io's API now rate-limits this environment (403), run `cargo info soroban-cost-estimator` from the maintainer machine to confirm the live version, and ensure `~/.cargo/credentials`/`CARGO_REGISTRY_TOKEN` exist for any future publish. Then update ROADMAP §"What remains" item 1.

### 7.2 P1 — contributor backlog: the 6 open issues (first sprint)

All verified genuinely open via `gh`; status annotations from the 2026-08-03/04 audit:

| # | Issue | Landed so far | Remaining ACs |
|---|-------|---------------|---------------|
| 1 | `feat(watch): graceful shutdown on SIGINT/SIGTERM` | Clean-exit on first signal (exit 0) | Second-signal force-exit (code 130); no partial snapshot mid-fetch; SIGINT integration test |
| 2 | `feat(report): populate read/write entries and bytes from the footprint` | Footprint decoding (live cross-check: 1/1 entries, 136 write bytes) | Automated test asserting `write_entries >= 1`; table-output verification; `minimal.wasm` zero-path test |
| 3 | `feat(estimate-all): parallelize per-function simulations` | `[i/N]` progress indicator | Parallel simulation (no `FuturesUnordered`/`join_all` in `src/`) |
| 4 | `feat(estimate): validate and coerce --arg values against contract-spec types` | Spec types parsed in `parse_contract_spec` | Spec-type coercion of `--arg` values (currently type-inferred: bool/i64/u64/string) |
| 5 | `feat(cache): prune stale estimates and add cache stats command` | — | Prune logic; `cache info` / `cache clear` subcommands (nothing in `cache.rs`/`cli.rs`) |
| 6 | `fix(watch): exponential backoff on RPC failures` | — | Backoff on failed polls (no "backoff" in `src/`) |

After Drips approval: set Medium/High complexity tiers in the dashboard (issues labeled only with GitHub labels default to Trivial).

**Suggested sprint order:** #4 and #6 are self-contained and high-value; #3 (parallelism) is a perf win; #5 (cache) rounds out the caching story; #1/#2 close their remaining ACs with tests.

### 7.3 P2 — small repo fix-ups (this review's findings)

- Commit (or discard) the working tree: `review.md` + `LICENSE-APACHE`.
- Delete stale local branches `docs-site`, `backup/docs-site`.
- Refresh `ROADMAP.md` (publish status, docs site, commit count 47, CI runs).
- Set the GitHub repo description.
- Add `--version` (clap one-liner) — needs a PR per branch protection.
- Revisit crate-level `#![allow(dead_code)]`.

### 7.4 P3 — stretch (post-submission)

- `estimate-all --fn <name>` subset filter; `contractmeta` section parsing for richer reports; banner/logo; contrib.rocks. Docs site extras **only if Drips ever asks** (it currently doesn't).

### 7.5 Ongoing duties (continuous — a drift alert is a release trigger)

- Re-run `config diff` after every Stellar protocol vote (P26 live May 2026; P27 vote July 2026 — roughly every 3–4 months). Stale cached estimates actively mislead users until addressed.
- Re-verify `ConfigSetting*` XDR shapes on ecosystem SDK bumps — a protocol upgrade occasionally restructures a config type, which would break decoding silently if untested.
- Keep posting fresh issues each Wave cycle — a maintainer who applies once and stops adding issues stops being useful to the program.

---

## 8. Prioritized action plan

| When | Action |
|------|--------|
| **Today (maintainer)** | Commit `review.md` + `LICENSE-APACHE`; delete stale branches; set repo description; refresh ROADMAP |
| **Before applying** | `cargo info soroban-cost-estimator` to confirm 0.1.0 is live; fresh pre-apply re-checks (approved list + 57blocks scope) |
| **This week (user)** | Apply to Drips Wave (browser action) |
| **First sprint (contributors)** | Issues #6 (backoff), #4 (arg coercion), #5 (cache), #3 (parallelism), then close #1/#2 remaining ACs with tests |
| **Continuous** | `config diff` after protocol votes; XDR re-verify on SDK bumps; fresh issues each cycle |

---

## Appendix — Prior verification audit (2026-08-03, condensed)

The previous `review.md` was a Drips-Standard verification audit. Its conclusions, all re-confirmed or superseded by this review:

| Check | Outcome |
|-------|---------|
| `cargo test --all` | ✅ 53 passed, 0 failed |
| `cargo clippy --all-targets --all-features` | ✅ exit 0, zero warnings |
| `cargo fmt --check` | ✅ clean |
| No `unwrap()`/`expect()` outside test modules | ❌→✅ one violation (`xdr_helper.rs:270`) **fixed in `38d6177`** and re-verified |
| CI green on `main` | ✅ 5/5 at audit time; 6/6 today |
| Branch protection | ✅ strict, `build` check, 1 review, enforce-admins |
| Issue backlog non-hollow | ✅ 6 issues, all ACs substantively unimplemented |
| License + topics | ✅ Apache-2.0; 5 topics (cli, developer-tooling, gas-estimation, soroban, stellar) |
| Published artifact matches repo | ✅ crates.io `0.1.0` ↔ Cargo.toml (version, repository, license, bins, description all match) |
| README differentiator vs 57blocks | ✅ live-fetched; mechanism-based claim accurate |
| Badges render | ✅ HTTP 200 ×4 |
| Install instructions work | ✅ `cargo install --path .` → both binaries |
| Live-testnet numbers match | ✅ CPU exact; fee within documented margin (17,606 vs 18,999, ~46k ledgers later) |

*End of review — 2026-08-05.*
