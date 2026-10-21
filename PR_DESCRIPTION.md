# feat(config, cache): explicit diff states, chronological snapshot ordering, and cache list/delete

Closes #514
Closes #515
Closes #516
Closes #517

---

## 1. Overview

This PR resolves four issues of the *Stellar Wave* backlog in one focused change set. All four
land in the two areas where the tool is least explicit today: **config diffs** and the **estimate
cache**.

| Issue | Title | Kind | What lands here |
|---|---|---|---|
| [#514](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues/514) | `test(config): verify snapshot ordering against embedded timestamps` | test | Snapshot listings and "latest snapshot" selection are ordered by the timestamp **embedded in the file name**, and the behaviour is pinned down with tests. |
| [#515](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues/515) | `feat(config): show missing and unknown settings explicitly in diffs` | enhancement | Config diffs model **presence** explicitly: *added*, *removed* and *unrecognized* settings are rendered distinctly from numeric value changes. |
| [#516](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues/516) | `feat(cache): inspect cached estimates by network, contract, and function` | enhancement | `cache list` becomes a full read-only, filtered, paginated and bounded view of the estimate cache. |
| [#517](https://github.com/Stellar-Cost-Labs/soroban-cost-estimator/issues/517) | `feat(cache): delete cached estimates using the existing query filters` | enhancement | New `cache delete` command: filtered removal with a dry-run count, an explicit confirmation step and transactional semantics. |

Everything in this PR is **offline**: no new network calls, no new dependencies, no protocol or
fee-math changes. All fee arithmetic stays integer-based, and no `unwrap()`/`expect()` was added
outside tests.

---

## 2. Motivation

### 2.1 Snapshot ordering was lexicographic, not chronological (#514)

`config snapshot` writes files named `{network}-{timestamp}.json`, where the timestamp's `:`
characters are replaced by `-` (so `2026-01-02T03:04:05+00:00` becomes
`2026-01-02T03-04-05+00-00.json`). The store picked the "latest" snapshot by comparing those
**file names** as plain strings, and the code even documented the assumption:

> *Filenames are `{network}-{timestamp}.json` where the timestamp is an RFC 3339 UTC instant with
> `:` replaced by `-`, so a plain lexicographic sort of the paths is also a chronological one.*

That is only accidentally correct. Two snapshots that are a few seconds apart but carry different
UTC offsets — e.g. `...T10-00-00+02-00` (08:00Z) and `...T09-00-00Z` — sort in the **wrong**
order lexicographically (`10` > `09`), so `config diff` could silently diff against the wrong
baseline. Files whose name carries no usable timestamp at all were interleaved at random,
depending on `read_dir` enumeration order.

### 2.2 Diffs conflated "missing" with "zero" and hid unknown settings (#515)

The diff engine represented a setting that is absent from one snapshot as the literal strings
`(missing)` / `(present)`, with no machine-readable distinction between:

* a numeric value change (`10 → 20`),
* a setting that appeared or disappeared (usually an older protocol or an incomplete fetch), and
* a setting this build does not model at all (a config section introduced by a newer protocol
  version) — which was **silently dropped** by serde.

That last case is the dangerous one: a snapshot written by a newer tool could diff as *"no
changes"*, hiding a possible pricing change.

### 2.3 The cache was a black box (#516) and a demolition site (#517)

`cache list` could only dump one network with no filters and no bound; `cache query` had filters
but no pagination; and the only way to remove entries was `cache clear`, which nukes an entire
network with no confirmation and no dry-run. There was no way to *see* what was stored across
networks, and no way to remove a single stale contract entry — an operator had to open the SQLite
database by hand.

---

## 3. What changed

### 3.1 `src/config_snapshot/store.rs` — chronological snapshot ordering (#514)

* **New `timestamp_from_filename(network, filename) -> Option<String>`** (public): reverses the
  `:` → `-` transformation of a snapshot file name and returns the original RFC 3339 timestamp.
  It returns `None` when the name does not follow `{network}-….json` or when the embedded
  timestamp is not a valid instant.
  * Handles `Z`, positive offsets (`+02-00`), negative offsets (`-05-00`, whose sign is swallowed
    by the `-` separator) and fractional seconds (which
    `chrono::Utc::now().to_rfc3339()` produces).
* **New internal `embedded_timestamp()`**: the parsed instant, normalised to UTC.
* **`list_snapshots_in()`** now returns the snapshots **oldest → newest**, ordered by the embedded
  timestamp instead of by path text. Files whose name carries no valid timestamp cannot be placed
  on the timeline, so they are listed **last** (ordered by name between themselves) instead of
  being interleaved at random. `config snapshot prune` / `--retain` build on this listing, so they
  inherit the corrected order.
* **`load_latest_snapshot()`** now selects the snapshot with the **newest embedded timestamp**.
  Files with an unusable timestamp are ignored by that selection. If *no* file carries a valid
  timestamp, the lexicographically greatest name is used as a documented fallback, so a
  hand-written snapshot stays reachable.

The behaviour is now *defined* rather than incidental:

| Situation | `list_snapshots` | `load_latest_snapshot` |
|---|---|---|
| Valid timestamps, same offset | chronological (oldest → newest) | newest by embedded timestamp |
| Valid timestamps, different offsets | chronological (UTC-normalised) | newest by embedded timestamp |
| Name matches but timestamp is invalid | listed **last** | ignored |
| No file has a valid timestamp | ordered by name | greatest name (fallback) |
| Unrelated file / other network prefix | ignored | ignored |

### 3.2 `src/config_snapshot/model.rs` — forward-compatible payload parsing (#515)

* **New `SnapshotPayload`**: a `ConfigSnapshot` **plus** the top-level fields this build does not
  model (`unknown_fields: BTreeMap<String, serde_json::Value>`).
* **`SnapshotPayload::parse(json)`** deserializes the known fields exactly as before (serde keeps
  ignoring unknown fields, so **forward compatibility is preserved**) and *captures* the unknown
  ones instead of dropping them on the floor.
* **`KNOWN_SNAPSHOT_FIELDS`** documents the recognised top-level field names in one place.
* Helpers: `from_snapshot()`, `unknown_field_names()`, `has_unknown_fields()`.

### 3.3 `src/config_snapshot/diff.rs` — explicit presence and unknown states (#515)

* **New `SettingState`** — the state of a setting on one side of a diff:
  * `Present(String)` — a known value,
  * `Missing` — the setting is **absent** (explicitly *not* the same as a zero value),
  * `Unknown(String)` — the setting is stored but not recognised by this build.
* **New `SettingChangeKind`** — `Value`, `Added`, `Removed`, `Unknown`.
* **`FieldDiff`** gained `change_kind`, `old_state` and `new_state`. The existing `old_value` /
  `new_value` strings are kept, so existing consumers and tests keep working.
* **`ConfigDiff`** gained `has_unknown_changes`.
* **New `diff_payloads(old, new)`** — `diff_snapshots` plus the unknown-field comparison:
  * an unknown field present on one side only, or with a different value, is reported as a
    `SettingChangeKind::Unknown` change;
  * unknown fields that are identical on both sides produce **no** change.
* **Conservative pricing status**: a transition where one side lacks a value keeps
  `is_pricing_change = true` (it is *not* downgraded to "no pricing impact"), because a missing
  setting is not a zero value and the tool cannot prove the price did not move. Unrecognised
  fields are treated the same way — their effect on pricing is unknown, so they are flagged
  rather than ignored.
* **`format_diff()`** now renders the four kinds distinctly:
  * `📈` numeric pricing change, `🔄` numeric non-pricing change,
  * `🆕` added, `🗑️` removed, `❓` unrecognized,
  * presence changes additionally print `State: missing → present`,
  * unknown settings print `Setting is not recognized by this build.` and the diff ends with
    `⚠️  Unrecognized settings detected — their effect on pricing is unknown.`
* **`format_diff_summary()`** appends `, N unknown` when unknown changes are present (the
  existing format is unchanged when there are none).
* The per-change rendering was extracted into `render_change()` / `exceeds_threshold()` helpers so
  the function stays within the repository's line-count lint budget.

### 3.4 `src/cache.rs` — one matching rule for queries, listings and deletions (#516, #517)

The repository already has the right abstraction: `CacheFilter` with `validate()` and
`matches()`, used by `query_cache`. This PR builds on it instead of duplicating it.

* **New `CacheListing`** (`entries`, `total`, `offset`, `limit`) and
  **`list_cached_estimates_paginated(filter, limit, offset)`** — a paginated listing across
  networks. `DEFAULT_LIST_LIMIT = 50`, `MAX_LIST_LIMIT = 1000` (the limit is always capped).
  The filter semantics are exactly those of `query_cache`.
* **New `count_cached_estimates(filter)`** — the match count used by the dry run.
* **New `delete_cached_estimates(filter)`** — selection *and* deletion happen inside **one SQLite
  transaction**; the rows are selected with `CacheFilter::matches` and deleted by primary key, so
  the reported count always matches the number of removed rows and a failure leaves the cache
  untouched. The existing write lock is reused, so deletions are serialised with other writers.
  Entries written by a *newer* tool (which cannot be migrated forward) are left alone rather
  than deleted blindly.
* `CacheFilter::validate()` is reused as-is, so a listing, a query and a deletion reject the same
  malformed input (non-hex hash, negative fee, `since` after `to`, unknown network, …).

### 3.5 `src/cli.rs` + `src/main.rs` — the CLI surface

```text
soroban-cost-estimator cache list [OPTIONS]
      --network <NETWORK>        Network to filter by (omit to list every network)
      --fn <FN>                  Filter by function name (exact match) [alias: --function]
      --wasm-hash <WASM_HASH>    Filter by WASM hash (prefix match, hexadecimal)
      --since <DATE/TIME>        Earliest timestamp or date [alias: --from]
      --to <TIMESTAMP>           Latest timestamp or date
      --older-than <DURATION>    Only entries older than this age ("30d", "12h", "3600")
      --limit <LIMIT>            Maximum number of entries to return [default: 50]
      --offset <OFFSET>          Number of matching entries to skip before listing [default: 0]
      --json                     Output the full cached-estimate records as a JSON array

soroban-cost-estimator cache delete [OPTIONS]
      --network <NETWORK>        Network to filter by (omit to match every network)
      --fn <FN>                  Filter by function name (exact match) [alias: --function]
      --wasm-hash <WASM_HASH>    Filter by WASM hash (prefix match, hexadecimal)
      --since <DATE/TIME>        Earliest timestamp or date [alias: --from]
      --to <TIMESTAMP>           Latest timestamp or date
      --older-than <DURATION>    Only entries older than this age ("30d", "12h", "3600")
      --all                      Delete every cached estimate, on every network
      --dry-run                  Report how many entries would be removed
  -y, --yes                      Skip the confirmation prompt
      --json                     Output as JSON instead of a summary
```

**`cache list`**

* Human-readable table by default, `--json` for machine consumption. The JSON output keeps its
  existing shape (a JSON array of records) and the records keep their stable field names:
  `network`, `wasm_hash`, `function`, `ledger`, `total_stroops`, `cpu_instructions`,
  `memory_bytes`, `timestamp`, `duration_ms`, `success`.
* **Argument payloads are never exposed** — the cache only ever stores a hash of the simulated
  arguments.
* The footer reports `Showing X of Y matching estimate(s) (offset Z).`

**`cache delete`**

* `--dry-run` prints `Would delete N cached estimate(s) — dry run, nothing removed.` and touches
  nothing.
* Without `--yes`, the command asks for confirmation, reusing the repository's existing
  `confirm()` helper. A non-interactive session (stdin is not a terminal) or a declined prompt
  aborts with `Aborted — no cached estimates deleted (pass --yes to skip the prompt).` and
  deletes nothing.
* A filter is **required**: `cache delete` without one is refused with
  `refusing to delete cached estimates without a filter — pass a filter (e.g. --network) or --all
  to remove every entry`. `--all` is the explicit opt-in for a whole-cache removal and still
  requires a confirmation.
* `--json` reports `matched`, `removed` and `dry_run`.
* `--older-than` is resolved into an inclusive upper timestamp bound; when combined with
  `--to`, the **earlier** bound wins. An unparseable duration is an error (never a silent
  fallback).

**Shared helpers**

* `build_cache_filter()` — one builder for both commands, so listing and deleting can never
  disagree about what a filter means.
* `parse_duration_secs()` — strict duration parsing (`s`/`m`/`h`/`d` suffixes or bare seconds).
* `filter_is_empty()` — the guard behind the "a filter is required" rule.
* `load_baseline_payload()` — loads the snapshot a diff compares against, keeping unknown fields.

### 3.6 `config diff` now uses the payload path

`cmd_config_diff` and `cmd_config_diff_against_previous` load their baseline through
`load_snapshot_payload_from_path()` / `load_latest_snapshot_payload()` /
`load_last_two_snapshot_payloads()` and diff with `diff_payloads()`, so a stored field this build
does not model is reported instead of silently ignored. The exit-code semantics, the
auto-save of a post-upgrade snapshot and the stale-cache cross-reference are unchanged.

---

## 4. Tests

All four issues' acceptance criteria are covered by automated tests. The whole suite is offline
and deterministic — no test contacts an RPC endpoint, and no test depends on the local timezone
or on filesystem enumeration order.

| File | New tests | Covers |
|---|---|---|
| `tests/config_history_tests.rs` | 9 | **#514** — timestamp round-trip (incl. positive/negative offsets and fractional seconds), rejection of invalid names, chronological ordering across hour/day/month/year boundaries, ordering with mixed offsets, invalid-timestamp files ignored by the selection and listed last, file-name fallback, unrelated files and other network prefixes ignored, `NoSnapshots` error. Uses a per-test temporary `HOME` and fixed timestamps. |
| `tests/config_diff_tests.rs` | 11 | **#515** — added/removed/value change kinds, presence states, conservative pricing status, payload parsing with unknown fields, unknown-field diffs (added / changed / identical), rendering of the new states and of the unknown marker, summary with unknown count. |
| `tests/cache_tests.rs` | 15 | **#516 / #517** — listing by network/function/hash, exact function match, hash prefix match, empty result, pagination (non-overlapping pages), limit cap, invalid hash rejection, time-range filter, counting, filtered deletion (matching vs non-matching, network isolation, query/delete agreement), invalid filter rejection on delete. |
| `tests/cli_tests.rs` | 18 | **#516 / #517** — help output for the new subcommands, empty listing, JSON shape and stable field names, argument payload not exposed, network/hash/combined filters, invalid hash and invalid timestamp errors, `--limit`, deletion refused without a filter, `--dry-run` count, `--yes` deletion, abort without confirmation, `--all`, JSON delete output, `--older-than`, invalid duration. |

**Test totals:** 810 tests, all green (289 unit + 93 cache + 185 CLI + 88 diff + 16 history + …).

---

## 5. Verification performed locally

Exactly the checks the issues and the repository CI require:

```console
$ cargo fmt --all -- --check
# clean

$ cargo clippy --workspace --all-targets -- -D warnings
# clean (no warnings, no errors)

$ cargo build --all-targets
# Finished `dev` profile

$ cargo run --bin gen_test_wasm
# Generated test WASM: tests/fixtures/minimal.wasm (44 bytes)
# Generated test WASM: tests/fixtures/zeroarg.wasm (45 bytes)

$ cargo test --workspace
# 810 tests, 0 failed
```

The CI workflow (`.github/workflows/ci.yml`) additionally runs the same commands on
`ubuntu-latest`, `macos-latest` and `windows-latest`, plus `cargo audit` and `cargo deny`.
No dependency was added or changed, so the audit/deny jobs are unaffected. The new tests are
cross-platform: they only rely on `HOME`/`USERPROFILE` redirection, temporary directories and
fixed timestamps.

Manual smoke test of the new commands:

```console
$ soroban-cost-estimator cache list
+---------------------------+----------+---------+-----------+---------------+--------+
| Timestamp                 | Function | Network | WASM Hash | Fee (stroops) | Ledger |
+=====================================================================================+
| 2026-10-21T12:17:20+00:00 | transfer | testnet | aabb00    | 1000          | 100    |
|---------------------------+----------+---------+-----------+---------------+--------+
| 2026-10-21T12:17:20+00:00 | approve  | testnet | aabb01    | 2000          | 101    |
|---------------------------+----------+---------+-----------+---------------+--------+
| 2020-01-01T00:00:00+00:00 | burn     | mainnet | aabb02    | 3000          | 102    |
+---------------------------+----------+---------+-----------+---------------+--------+

Showing 3 of 3 matching estimate(s) (offset 0).

$ soroban-cost-estimator cache delete --network testnet --dry-run
Would delete 2 cached estimate(s) — dry run, nothing removed.

$ soroban-cost-estimator cache delete --network testnet --yes
Deleted 2 cached estimate(s).

$ soroban-cost-estimator cache delete --yes
Error: operation failed: refusing to delete cached estimates without a filter — pass a filter (e.g. --network) or --all to remove every entry
# exit code 1
```

---

## 6. Acceptance criteria checklist

### #514 — `test(config): verify snapshot ordering against embedded timestamps`

- [x] Cover snapshots captured across hour/day/year boundaries and verify oldest-to-newest ordering.
- [x] Define behavior for files whose names match but whose embedded timestamp is invalid
      (ignored by the selection, listed last, name-order fallback).
- [x] Ensure unrelated files and other network prefixes are ignored.
- [x] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` all pass.
- [x] PR references the issue with `Closes #514`.

### #515 — `feat(config): show missing and unknown settings explicitly in diffs`

- [x] Represent added, removed and unknown setting states separately in diff results
      (`SettingState`, `SettingChangeKind`, `old_state`/`new_state`/`change_kind`).
- [x] Ensure pricing-change status is conservative when one side lacks a value (absence is never
      mapped to a zero value; the transition stays pricing-relevant and is rendered explicitly).
- [x] Add tests for missing optional sections and unknown serialized fields.
- [x] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` all pass.
- [x] PR references the issue with `Closes #515`.

### #516 — `feat(cache): inspect cached estimates by network, contract, and function`

- [x] Expose the existing cache query filters through a CLI command with pagination and a bounded
      result limit (`cache list`, `--limit`/`--offset`, capped at 1000).
- [x] Support table and JSON output with stable field names and no argument values or secrets.
- [x] Test combined filters, empty results and invalid date/hash inputs.
- [x] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` all pass.
- [x] PR references the issue with `Closes #516`.

### #517 — `feat(cache): delete cached estimates using the existing query filters`

- [x] Add a dry-run count and an explicit confirmation step before filtered deletion.
- [x] Apply the same validation and matching semantics used by cache queries (one shared
      `CacheFilter::validate()` / `CacheFilter::matches()` implementation, one shared filter
      builder).
- [x] Test that matching entries are removed and nonmatching entries remain unchanged.
- [x] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` all pass.
- [x] PR references the issue with `Closes #517`.

---

## 7. Scope, compatibility and risk

**Backwards compatible**

* No public API was removed. `diff_snapshots`, `query_cache`, `query_estimates`,
  `list_snapshots`, `load_latest_snapshot` and `load_snapshot_from_path` keep their signatures
  and semantics.
* The existing `cache query`, `cache clear`, `cache export`, `cache stats`, `cache prune` and
  `cache verify` commands are untouched.
* `FieldDiff`/`ConfigDiff` only **gained** fields; `old_value`/`new_value` keep their previous
  strings, so existing JSON consumers keep working. The diff's JSON output gains
  `change_kind`, `old_state`, `new_state` and `has_unknown_changes`.
* `cache list --json` keeps its shape (a JSON array of records).
* The `config diff` exit code (1 on pricing changes), the auto-save of a post-upgrade snapshot and
  the stale-cache cross-reference are unchanged.

**One intentional behaviour change (documented in the CHANGELOG)**

* `cache list` now lists entries from **every** network unless `--network` is given; it used to
  default to `testnet` only. This matches `cache query`, which has always been network-optional,
  and makes the command usable as the cache-wide audit the issue asks for. Scripts that relied on
  the old default can pass `--network testnet` explicitly.

**Deliberately unchanged (per the issues' "Out of scope")**

* The snapshot file-name format is unchanged (`{network}-{timestamp}.json`).
* No new snapshot search command was added.
* No new protocol `ConfigSetting` type is implemented — unknown settings are *reported*, not
  interpreted.
* Cross-network diff behaviour is unchanged.
* No cache statistics feature was added (`cache stats` already exists).
* Cache key and eviction semantics are unchanged.
* No automatic age-based pruning was added — `--older-than` is an explicit, one-shot filter.
* The whole-cache deletion still requires an explicit `--all` **and** a confirmation.

**Risk assessment: low.** The behaviour changes are confined to (a) the ordering of snapshot
listings, which becomes *more* correct, (b) the rendering of config diffs, which gains explicit
states, and (c) two brand-new read-mostly/destructive commands that are opt-in and confirmed. No
network protocol, fee-math or cache-key logic was touched.

---

## 8. Documentation

* `README.md` — new `cache list` and `cache delete` sections; `config diff` now documents the
  added/removed/unknown distinction.
* `docs/commands/cache-list.md` — full reference for the listing command.
* `docs/commands/cache-delete.md` — full reference for the deletion command, including the
  confirmation and dry-run semantics.
* `docs/commands.md` — both commands added to the command reference and the table of contents.
* `docs/commands/config-diff.md` — documents the new presence/unknown rendering.
* `docs/SUMMARY.md` — the two new pages are linked.
* `CHANGELOG.md` — `Added` entries for the two commands and the diff/ordering behaviour, plus a
  `Changed` entry for the `cache list` network default.

---

## 9. Files changed

```text
 CHANGELOG.md                       |  25 +-
 README.md                          |  55 ++++++
 docs/SUMMARY.md                    |   2 +
 docs/commands.md                   | 101 +++++++++++
 docs/commands/cache-delete.md      | 105 ++++++++++ (new)
 docs/commands/cache-list.md        | 116 ++++++++++ (new)
 docs/commands/config-diff.md       |  10 +
 src/cache.rs                       | 130 ++++++++++++
 src/cli.rs                         |  92 +++++++++
 src/config_snapshot/diff.rs        | 320 +++++++++++++++++++++++++++++--
 src/config_snapshot/model.rs       |  79 ++++++
 src/config_snapshot/store.rs       | 186 ++++++++++++--
 src/main.rs                        | 230 +++++++++++++-
 tests/cache_tests.rs               | 315 +++++++++++++++++++++
 tests/cli_tests.rs                 | 400 ++++++++++++++++++++++++++++++-
 tests/config_diff_tests.rs         | 239 ++++++++++++++++++
 tests/config_history_tests.rs      | 300 ++++++++++++++++++++++
```

---

## 10. Reviewer notes / things to look at

1. **`src/config_snapshot/store.rs`** — `timestamp_from_filename` is the only place that knows
   about the `:` → `-` file-name encoding; it is the single source of truth for ordering, and it
   is covered by unit tests for every offset shape.
2. **`src/cache.rs`** — `CacheFilter::matches` remains the single source of truth for cache
   matching; `query_cache`, `list_cached_estimates_paginated`, `count_cached_estimates` and
   `delete_cached_estimates` all delegate to it, so a filter selects exactly the same entries no
   matter which command uses it.
3. **`src/main.rs`** — `build_cache_filter` is shared by `cache list` and `cache delete`, so the
   two commands can never disagree about what a filter means.
4. **Presence vs. unknown** — `SettingState::Missing` (absent) and `SettingState::Unknown`
   (present but not modelled) are deliberately different: the first usually means "older
   protocol", the second means "newer protocol". Both are treated conservatively with respect to
   pricing.
5. **Deletion safety** — the filter requirement, the dry-run, the confirmation prompt and the
   single-transaction delete are independent layers; none of them can be bypassed by accident.
6. `cache clear` was intentionally left untouched (its behaviour is covered by its own issue).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
