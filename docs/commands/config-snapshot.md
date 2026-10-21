# `config snapshot`

Fetch all six `ConfigSetting` ledger entries, decode them via XDR, timestamp
them, and save to disk.

## Flags

```
Usage: soroban-cost-estimator config snapshot [OPTIONS]
       soroban-cost-estimator config snapshot <COMMAND>

Options:
      --network <NETWORK>  Network to fetch config from [default: testnet]
      --out <OUT>          Explicit output path (defaults to ~/.soroban-cost-estimator/snapshots/)
      --json               Print the snapshot as JSON instead of the summary lines
      --retain <COUNT>     Keep only the N most recent snapshots for the network
      --skip-unchanged     Do not write a new snapshot when the configuration is unchanged
  -h, --help               Print help

Commands:
  list    List saved snapshots: filename, network, timestamp, ledger sequence
  prune   Delete stored snapshots older than a number of days
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
- `--retain <COUNT>` runs a retention pass once the new snapshot is safely on
  disk (see [Retention](#retention)).
- The file declares its schema version (see
  [Schema versioning](#schema-versioning)); files written by a newer release
  are rejected with the file name and an upgrade hint instead of being
  misread.
- `--skip-unchanged` compares the fetched configuration with the latest stored
  snapshot and writes nothing when they match (see
  [Skipping unchanged captures](#skipping-unchanged-captures)).

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

## Schema versioning

Every new snapshot starts with an explicit schema marker:

```json
{
  "schema_version": 1,
  "network": "testnet",
  "timestamp": "2026-09-24T21:04:11.016322+00:00",
  "ledger": 3470630
}
```

- Snapshots saved before the marker existed have no `schema_version` key and
  are treated as **version 1**, the documented initial schema. Reading one
  never rewrites it: the file keeps its original shape until you explicitly
  save a new snapshot.
- A file that declares a version newer than this build understands is
  rejected across every read path (`list`, `diff`, `validate`, `--against`,
  retention) with a message naming the file and recommending an upgrade:

  ```text
  Error: failed to load snapshot: /home/you/.soroban-cost-estimator/snapshots/testnet-2026-10-01T00-00-00+00-00.json declares schema version 2, but this build supports up to 1 (upgrade soroban-cost-estimator to read it)
  ```

  The file is left untouched, so the newer release can keep using it.
- `config snapshot validate` reports the same message and marks the file
  invalid. `config export` skips such files with a warning rather than
  downgrading them into a bundle it cannot represent faithfully (and
  `config import` rejects a bundle that contains one).

## Skipping unchanged captures

Scheduled collection can run more often than network pricing changes. Add
`--skip-unchanged` to avoid writing a duplicate file each time:

```bash
soroban-cost-estimator config snapshot --network testnet --skip-unchanged --retain 30
```

After fetching, the tool compares the **decoded data fields** — network,
protocol version, and all six config settings — against the latest snapshot
for that network. The timestamp, the snapshot's last-modified ledger, and the
schema version are deliberately ignored, so capturing the same values again
does not count as a change. A real fee change, a protocol-version bump, or a
different network always writes a new file.

When nothing changed:

```text
Configuration unchanged; no new snapshot written.
Latest:  /home/you/.soroban-cost-estimator/snapshots/testnet-2026-09-24T21-04-11.016322+00-00.json
Network: testnet
Ledger:  3470630
Time:    2026-09-25T09:00:00.000000+00:00
```

- **Exit status is `0`** whether a snapshot was written or skipped. An
  unchanged configuration is a successful scheduled run, not a drift error;
  use `config diff` when you want a non-zero exit on pricing changes.
- With `--json` and `--format json`, the skipped result is a small document
  instead of a snapshot, so automation can branch on `status`:

  ```json
  {
    "status": "unchanged",
    "network": "testnet",
    "ledger": 3470630,
    "timestamp": "2026-09-25T09:00:00.000000+00:00",
    "latest_snapshot": "/home/you/.soroban-cost-estimator/snapshots/testnet-2026-09-24T21-04-11.016322+00-00.json"
  }
  ```

  `--format csv` uses the header `status,network,ledger,timestamp,path` and
  `--format markdown` adds a `Status | unchanged` row.
- The first snapshot for a network is always written, and an explicit `--out`
  path is always written — the flag exists for managed snapshot history, not
  for redirecting a single capture.
- `--retain` still runs after an unchanged capture, so a directory that grew
  before this mode was enabled is still trimmed.

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

## Retention

A scheduled `config snapshot` (or a long-running `watch`) can leave hundreds of
snapshot files behind. Two options keep the directory bounded without you ever
deleting anything by hand.

### `--retain <COUNT>` — keep the newest N

```bash
soroban-cost-estimator config snapshot --network testnet --retain 10
```

Fetches and saves as usual, then deletes the oldest snapshots until only the
10 most recent remain:

```text
Config snapshot saved to: /home/you/.soroban-cost-estimator/snapshots/testnet-2026-09-24T21-04-11.016322+00-00.json
Network: testnet
Ledger:  3470630
Time:    2026-09-24T21:04:11.016322+00:00
Pruned 2 snapshot(s) for testnet (--retain 10).
  - /home/you/.soroban-cost-estimator/snapshots/testnet-2026-08-04T07-15-38.487702259+00-00.json
  - /home/you/.soroban-cost-estimator/snapshots/testnet-2026-08-15T09-02-44.119283+00-00.json
```

Retention runs **after** the save, so a failed fetch or write never costs you
the older snapshots it would have pruned. With `--json`, the save document goes
to stdout unchanged and the retention line is logged to stderr, so stdout stays
a single parseable document.

### `config snapshot prune --older-than <DAYS>` — drop stale files

```bash
soroban-cost-estimator config snapshot prune --network testnet --older-than 30
```

Deletes every `testnet` snapshot recorded more than 30 days ago. This is a pure
file operation — no RPC call — so it is safe in a cron job on a machine that
cannot reach the network. Add `--json` for a machine-readable summary:

```json
{
  "network": "testnet",
  "policy": "--older-than 30d",
  "retain_count": null,
  "retain_days": 30,
  "pruned_count": 2,
  "pruned": [
    "/home/you/.soroban-cost-estimator/snapshots/testnet-2026-06-26T21-35-45+00-00.json",
    "/home/you/.soroban-cost-estimator/snapshots/testnet-2026-08-15T21-35-45+00-00.json"
  ],
  "remaining": 2
}
```

`--older-than 0` deletes everything except the newest snapshot.

### The latest snapshot is never deleted

Every rule protects the newest snapshot for the network, however old it is and
however small `--retain` is (`--retain 0` behaves like `--retain 1`). A
retention run must never leave you with nothing to diff against, so
[`config diff --against-previous`](config-diff.md#against-previous--comparing-two-snapshots-on-disk)
always has a pair to compare.

Because `prune` never fetches a snapshot, it is deliberately mutually exclusive
with the fetching flags — `config snapshot --retain 3 prune --older-than 30` is
rejected rather than silently ignoring `--retain`.

A snapshot written with `--out` lives outside the managed directory, so it is
neither counted by `--retain` nor deleted by `prune`.
