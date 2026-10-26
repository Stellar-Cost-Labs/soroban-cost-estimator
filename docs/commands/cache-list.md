# `cache list`

Audit what is stored in the estimate cache without opening the SQLite
database by hand. The listing is read-only and never modifies the cache.

## Flags

```
Usage: soroban-cost-estimator cache list [OPTIONS]

Options:
      --network <NETWORK>      Network to filter by (omit to list every network)
      --fn <FN>                Filter by function name (exact match) [aliases: --function]
      --wasm-hash <WASM_HASH>  Filter by WASM hash (prefix match, hexadecimal)
      --since <DATE/TIME>      Earliest timestamp or date [aliases: --from]
      --to <TIMESTAMP>         Latest timestamp or date
      --older-than <DURATION>  Only entries older than this age (e.g. "30d", "12h", "3600")
      --limit <LIMIT>          Maximum number of entries to return [default: 50]
      --offset <OFFSET>        Number of matching entries to skip before listing [default: 0]
      --json                   Output the full cached-estimate records as a JSON array
      --help                   Print help
```

## Behavior

- Without `--network`, entries from **every** network are listed.
- `--fn` matches the function name exactly, `--wasm-hash` matches a
  hexadecimal prefix against the SHA-256 of the WASM bytes.
- `--since`/`--to` bound the entry timestamp; both accept RFC 3339
  (`2026-01-01T00:00:00Z`) and plain dates (`2026-01-01`). `--older-than` is a
  shorthand for an upper bound relative to now (`30d`, `12h`, `3600`, …).
- Filters are combined with logical AND. An invalid timestamp or a
  non-hexadecimal hash prefix is rejected with a clear error instead of
  silently matching nothing.
- Results are ordered newest-first and paginated: `--limit` is capped at
  1000 entries, `--offset` skips matches.
- The cache only ever stores a **hash** of the simulated arguments, never the
  argument values themselves.
- **No network calls are made** — this is pure local SQLite I/O.

## Example

```bash
soroban-cost-estimator cache list --network testnet
```

```text
+--------------------+-----------+------------+--------------+---------+--------+
| Timestamp          | Function | Network    | WASM Hash    | Fee     | Ledger |
+--------------------+-----------+------------+--------------+---------+--------+
| 2026-10-01T00:00Z | transfer | testnet    | aabb00       | 1000    | 100    |
+--------------------+-----------+------------+--------------+---------+--------+
| 2026-10-01T00:00Z | approve  | testnet    | aabb01       | 2000    | 101    |
+--------------------+-----------+------------+--------------+---------+--------+

Showing 2 of 2 matching estimate(s) (offset 0).
```

JSON output (a JSON array of records with stable field names):

```bash
soroban-cost-estimator cache list --json
```

```json
[
  {
    "args_hash": "e3b0c442...",
    "cpu_instructions": 500,
    "duration_ms": 12,
    "function": "transfer",
    "ledger": 100,
    "memory_bytes": 100,
    "network": "testnet",
    "schema_version": 3,
    "success": true,
    "timestamp": "2026-10-01T00:00:00+00:00",
    "total_stroops": 1000,
    "wasm_hash": "aabb00"
  }
]
```

An empty result prints a friendly message instead of an empty table:

```text
No cached estimates match the filters.
```
