# Caching

Every `estimate` (and every per-function `estimate-all` simulation) saves its
result to a local cache. The cache is what lets `config diff` tell you
*which* of your past estimates are now stale after a network pricing change —
without it, a changed rate is just a curiosity.

There are two distinct kinds of caching here, and they are easy to confuse:

- **On-disk estimate cache** — persists between runs, and is the subject of
  most of this page.
- **In-memory config cache** — lives only for one command run, and is
  described in [In-memory config cache](#in-memory-config-cache) below.

## Where results live

All data lives in your home directory:

| Path | Purpose |
|------|---------|
| `~/.soroban-cost-estimator/cache.db` | Past `estimate` results, in one SQLite database |
| `~/.soroban-cost-estimator/cache/` | Legacy per-entry JSON cache; only exists after upgrading from an older release, and is imported then removed on first use |
| `~/.soroban-cost-estimator/snapshots/` | Timestamped config snapshots (JSON) |

SQLite replaced the old one-file-per-estimate store because a directory of
thousands of JSON files means inode overhead and an O(n) directory scan for
every lookup, listing, and stats call. The current schema keys rows by
`(wasm_hash, function, args_hash)` and adds secondary indexes on
`(wasm_hash, function)`, `(network, timestamp)`, the full
`(wasm_hash, function, network, timestamp)` combination, and `last_accessed`
(the eviction order). Writes are transactional, so an interrupted run can
never leave a half-written entry behind.

## How entries are keyed

Each cached estimate is keyed by three things:

1. **WASM hash** — the SHA-256 of the contract's `.wasm` bytes (hex). The
   same contract compiled twice yields the same hash; any rebuild changes it.
2. **Function** — the invoked function name, or `(wasm upload)` for the
   upload-only simulation (an `estimate` without `--fn`).
3. **Args hash** — the SHA-256 of the joined raw `--arg` values.

The tuple is the table's primary key. The fixture contract's deployed WASM
carries the hash
`ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd`, which is
why the tool's cached estimate for the deployed contract matches the fixture
exactly.

## What a cache entry contains

`cache export` serializes every row as the same JSON shape the old per-file
cache used, so backups stay portable:

```json
{
  "schema_version": 3,
  "wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "function": "increment",
  "args_hash": "…",
  "network": "testnet",
  "ledger": 3961551,
  "total_stroops": 17122,
  "cpu_instructions": 524389,
  "memory_bytes": 0,
  "timestamp": "2026-08-04T07:…Z",
  "duration_ms": 412,
  "success": true
}
```

The `ledger` field is the sequence number the simulation ran against — that
is the key to staleness detection.

## Schema versioning

Every entry records a `schema_version`. Rows written by older releases are
migrated forward automatically: unversioned legacy entries are treated as
version 1, version 2 added `duration_ms`/`success`, and version 3 (current)
added LRU access tracking. A row whose version is *newer* than the tool
understands is rejected with an error naming the version and suggesting an
upgrade (or `cache clear`), rather than being silently misread. Database
schema changes are applied in place on open, so upgrading the tool never
requires deleting your cache.

## Legacy JSON import

Releases before the SQLite backend wrote one JSON file per estimate into
`~/.soroban-cost-estimator/cache/`. On the first run after upgrading, the tool
imports every file into `cache.db` (existing rows win), deletes the imported
files, and removes the directory. Unparseable files are renamed to
`*.json.rejected` instead of being deleted so nothing is lost.

## Size limits and LRU eviction

Cache growth is bounded by two global quotas — `--max-cache-size-mb`
(default 50) and `--max-cache-entries` (default 10,000). When a write pushes
the cache past a quota, the least-recently-accessed rows are deleted until
usage falls below 90% of the quota. Recency is a `last_accessed` timestamp
updated on every cache read, so entries you keep reusing survive while stale
ones are dropped first. `cache prune` runs the same pass on demand, and
`cache stats` shows current usage against the quotas.

## The stale-estimate cross-reference

`config diff` (and every `watch` poll) lists the cached estimates for the
target network and compares each one's `ledger` against the *current*
network config's ledger. Any estimate recorded at an earlier ledger is
reported as potentially stale:

```
  1 cached estimate(s) from earlier ledger(s) — may be stale:
    - (wasm upload) @ ledger 0 (current: 3470630)
```

This is deliberately a *warning*, not an automatic invalidation: a fee can be
out of date because the pricing model changed, but it can also merely be old.
The tool cannot know which, so it names the estimates and leaves the
re-estimation to you.
