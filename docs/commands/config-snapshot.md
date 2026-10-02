# `config snapshot`

Fetch all six `ConfigSetting` ledger entries, decode them via XDR, timestamp
them, and save to disk.

## Flags

```
Usage: soroban-cost-estimator config snapshot [OPTIONS]
       soroban-cost-estimator config snapshot <COMMAND>

Commands:
  prune  Delete stored snapshots older than a number of days

Options:
      --network <NETWORK>  Network to fetch config from [default: testnet]
      --out <OUT>          Explicit output path (defaults to ~/.soroban-cost-estimator/snapshots/)
      --json               Print the snapshot as JSON instead of the summary lines
      --retain <COUNT>     Keep only the N most recent snapshots for the network
  -h, --help               Print help
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
