# `config snapshot`

Fetch all six `ConfigSetting` ledger entries, decode them via XDR, timestamp
them, and save to disk.

## Flags

```
Usage: soroban-cost-estimator config snapshot [OPTIONS]

Options:
      --network <NETWORK>  Network to fetch config from [default: testnet]
      --out <OUT>          Explicit output path (defaults to ~/.soroban-cost-estimator/snapshots/)
      --json               Print the snapshot as JSON instead of the summary lines
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
the fee rates used by `estimate` (see [Resource Fees](concepts/resource-fees.md)):

- `contract_compute` — `fee_rate_per_instructions_increment`, memory limits
- `contract_ledger_cost` — read/write entry fees, per-KB disk fees, rent rates
- `contract_historical_data` — `fee_historical1_kb`
- `contract_events` — `fee_contract_events1_kb`
- `contract_bandwidth` — `fee_tx_size1_kb`
- `state_archival` — TTLs, rent-rate denominators, eviction policy

Take a fresh snapshot after every protocol vote and keep them around:
[`config diff`](config-diff.md) compares the current configuration against
your most recent snapshot.
