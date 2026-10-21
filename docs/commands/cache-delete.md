# `cache delete`

Delete the cached estimates matching a filter — a targeted alternative to
clearing a whole network with `cache clear`.

## Flags

```
Usage: soroban-cost-estimator cache delete [OPTIONS]

Options:
      --network <NETWORK>      Network to filter by (omit to match every network)
      --fn <FN>                Filter by function name (exact match) [aliases: --function]
      --wasm-hash <WASM_HASH>  Filter by WASM hash (prefix match, hexadecimal)
      --since <DATE/TIME>      Earliest timestamp or date [aliases: --from]
      --to <TIMESTAMP>         Latest timestamp or date
      --older-than <DURATION>  Only entries older than this age (e.g. "30d", "12h", "3600")
      --all                    Delete every cached estimate, on every network (cannot be combined with a filter)
      --dry-run                Report how many entries would be removed without deleting them
  -y, --yes                    Skip the confirmation prompt (required in non-interactive sessions)
      --json                   Output as JSON instead of a human-readable summary
      --help                   Print help
```

## Behavior

- The filters use **exactly** the same matching rules as `cache list` and
  `cache query` — the same `CacheFilter` validation and the same
  `CacheFilter::matches` implementation — so the entries reported by
  `--dry-run` are the entries that get removed.
- `--dry-run` prints the number of matching entries and removes nothing.
- Without `--yes`, the command asks for confirmation before deleting. In a
  non-interactive session (stdin is not a terminal) the prompt cannot be
  answered, so the command aborts without deleting anything.
- At least one filter is required. `cache delete` without a filter is
  refused unless `--all` is given, so a stray invocation cannot wipe the
  cache. `--all` is the explicit opt-in for a whole-cache removal and still
  requires a confirmation.
- `--all` means "every cached estimate on every network". It **cannot be
  combined with a filter**: the combination is rejected with
  `--all cannot be combined with a filter`, so a whole-cache deletion can
  never be silently narrowed by a stray `--network`.
- Selection and deletion run inside a single SQLite transaction: the
  reported count always matches the number of removed rows, and a failure
  leaves the cache untouched.
- **No network calls are made** — this is pure local SQLite I/O.

## Example

Count first, then delete:

```bash
soroban-cost-estimator cache delete --network testnet --dry-run
```

```text
Would delete 2 cached estimate(s) — dry run, nothing removed.
```

Delete with the interactive confirmation:

```bash
soroban-cost-estimator cache delete --network testnet
```

```text
Delete 2 cached estimate(s)? [y/N] y
Deleted 2 cached estimate(s).
```

Delete without a prompt (for scripts and CI):

```bash
soroban-cost-estimator cache delete --network testnet --yes
```

```text
Deleted 2 cached estimate(s).
```

JSON output:

```bash
soroban-cost-estimator cache delete --network testnet --yes --json
```

```json
{
  "dry_run": false,
  "matched": 2,
  "removed": 2
}
```

Remove everything that is older than 90 days:

```bash
soroban-cost-estimator cache delete --older-than 90d --dry-run
```
