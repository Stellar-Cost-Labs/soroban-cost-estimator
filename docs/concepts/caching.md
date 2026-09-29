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

All data lives in your home directory — no database required:

| Directory | Purpose |
|-----------|---------|
| `~/.soroban-cost-estimator/cache/` | Past `estimate` results, one JSON file each |
| `~/.soroban-cost-estimator/snapshots/` | Timestamped config snapshots (JSON) |

## How entries are keyed

Each cached estimate is keyed by three things:

1. **WASM hash** — the SHA-256 of the contract's `.wasm` bytes (hex). The
   same contract compiled twice yields the same hash; any rebuild changes it.
2. **Function** — the invoked function name, or `(wasm upload)` for the
   upload-only simulation (an `estimate` without `--fn`).
3. **Args hash** — the SHA-256 of the joined raw `--arg` values.

The file name is `{wasm_hash}-{function}-{args_hash}.json` in the cache
directory. The fixture contract's deployed WASM carries the hash
`ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd`, which is
why the tool's cached estimate for the deployed contract matches the fixture
exactly.

## What a cache entry contains

```json
{
  "wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "function": "increment",
  "args_hash": "…",
  "network": "testnet",
  "ledger": 3961551,
  "total_stroops": 17122,
  "cpu_instructions": 524389,
  "memory_bytes": 0,
  "timestamp": "2026-08-04T07:…Z"
}
```

The `ledger` field is the sequence number the simulation ran against — that
is the key to staleness detection.

## In-memory config cache

Within a single command run, the network's config settings are cached in
memory. This is not a durability feature — nothing is written to disk, and
nothing survives the process.

`estimate-all` needs the same `ConfigSettingContract*` entries to price every
function it evaluates. Fetching them per function would mean repeating the
same `getLedgerEntries` work N times over. Instead the six settings are fetched
once, in a single batched call, and shared by reference with every fee
evaluation in that run:

| Functions evaluated | Config fetches before | Config fetches now |
|---|---|---|
| 1 | 1 | 1 |
| N | N | 1 |

Two consequences worth being explicit about:

- **A fresh command still sees fresh prices.** Because the cache is per-run,
  re-running the command after a protocol upgrade re-reads the network and
  picks up the new rates. This is deliberate — a fee estimate that silently
  reused a stale rate from an earlier run would be misleading, which is the
  exact problem the on-disk snapshot/diff tooling exists to surface.
- **A failed fetch is not cached.** A transient RPC error is retried by the
  next caller rather than pinning the run to a missing config. If the config
  genuinely cannot be read, the affected rates fall back to 0 and the tool
  warns on stderr — a silent zero rate would understate the non-refundable
  fee.

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
