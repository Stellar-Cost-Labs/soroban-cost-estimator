# `config snapshot`

Fetch all six `ConfigSetting` ledger entries, decode them via XDR, timestamp
them, and save to disk.

## Flags

```
Usage: soroban-cost-estimator config snapshot [OPTIONS] [COMMAND]

Options:
      --network <NETWORK>  Network to fetch config from [default: testnet]
      --out <OUT>          Explicit output path (defaults to ~/.soroban-cost-estimator/snapshots/)
      --json               Print the snapshot as JSON instead of the summary lines
  -h, --help               Print help

Commands:
  delete  Delete a saved snapshot file, or purge every snapshot older than N days
  diff    Compare two saved snapshot files offline, without any network calls
```

## Behavior

- Fetches all six `ConfigSetting*` entries in **one batched**
  `getLedgerEntries` RPC call.
- Decodes each entry's XDR (`stellar-xdr` 27.x, big-endian) into a typed
  snapshot model.
- Saves the snapshot as
  `~/.soroban-cost-estimator/snapshots/{network}-{timestamp}.json` — the
  timestamp makes every snapshot a versioned artifact.
- `--json` also prints the full snapshot as JSON (it still saves it).
- `--out` writes to an explicit path instead of the default directory.

## Example

```bash
soroban-cost-estimator config snapshot --network testnet
```

Actual output from a live testnet run:

```text
Config snapshot saved to: /home/you/.soroban-cost-estimator/snapshots/testnet-2026-08-04T07-15-38.487702259+00-00.json
Network: testnet
Ledger:  3470630
Time:    2026-08-04T07:15:38.487702259+00:00
```

The printed `Ledger` is the last ledger at which the config entries were
modified on-chain — it is *not* the network's current ledger, and that is
intentional: it is the ledger against which stale-cache checks are made.

## What you get

The snapshot JSON contains the decoded values of all six settings, including
the fee rates used by `estimate` (see [Resource Fees](../concepts/resource-fees.md)):

- `contract_compute` — `fee_rate_per_instructions_increment`, memory limits
- `contract_ledger_cost` — read/write entry fees, per-KB disk fees, rent rates
- `contract_historical_data` — `fee_historical1_kb`
- `contract_events` — `fee_contract_events1_kb`
- `contract_bandwidth` — `fee_tx_size1_kb`
- `state_archival` — TTLs, rent-rate denominators, eviction policy

Take a fresh snapshot after every protocol vote and keep them around:
[`config diff`](config-diff.md) compares the current configuration against
your most recent snapshot.

## Deleting snapshots

`config snapshot delete` removes saved snapshots. It never touches the
network, and it is the counterpart to the accumulating
`~/.soroban-cost-estimator/snapshots/` directory.

```
Usage: soroban-cost-estimator config snapshot delete [OPTIONS] [FILENAME]

Arguments:
  [FILENAME]  Snapshot filename (or path) to delete

Options:
      --older-than <DAYS>  Delete snapshots older than this many days
      --dry-run            Show which files would be removed without deleting anything
  -y, --yes                Skip the confirmation prompt (required in non-interactive sessions)
      --network <NETWORK>  Restrict --older-than to a single network's snapshots
  -h, --help               Print help
```

- `config snapshot delete testnet-2026-01-01T00-00-00+00-00.json` deletes one
  file, resolved either as given (a path) or as a filename inside the
  snapshots directory.
- `config snapshot delete --older-than 30` purges every snapshot whose recorded
  `timestamp` is more than 30 days old, across all networks (add `--network`
  to scope it).
- `--dry-run` lists the affected files and deletes nothing.
- Deleting a snapshot that does not exist is an error, so a typo never looks
  like a successful cleanup.
- Snapshots whose timestamp cannot be parsed are skipped rather than deleted.

## Offline snapshot diff

`config snapshot diff <SNAPSHOT_A> <SNAPSHOT_B>` compares two saved snapshots
without contacting the network at all, which makes it suitable for comparing
historical snapshots (e.g. before and after a protocol upgrade) or for CI.

```
Usage: soroban-cost-estimator config snapshot diff [OPTIONS] <SNAPSHOT_A> <SNAPSHOT_B>

Arguments:
  <SNAPSHOT_A>  First (older) snapshot file to compare
  <SNAPSHOT_B>  Second (newer) snapshot file to compare

Options:
      --json   Output as JSON instead of a human-readable diff
  -h, --help   Print help
```

The output is the same field-by-field diff used by
[`config diff`](config-diff.md), with pricing changes highlighted. Exit codes
match `config diff`:

- `0` — no pricing changes
- `1` — pricing changes detected (or either file could not be read/parsed)

The error message names the offending file, since two files are read in one run.
