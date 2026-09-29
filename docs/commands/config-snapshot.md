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
Ledger:  4635341
Time:    2026-08-04T07:15:38.487702259+00:00
```

The printed `Ledger` is the network's **current** ledger at the moment of the
fetch, as reported by the node's `latestLedger`. It may trail Horizon or
`getLatestLedger` by a ledger or two, which is normal — the two endpoints close
ledgers independently.

This is deliberately *not* the ledger at which the config entries were last
modified. Config settings only change on protocol-governance events, so that
value sits frozen between upgrades and would leave every snapshot reporting the
same long-stale ledger. If you need the per-setting modification ledgers, they
are kept per setting in `settings_last_modified`:

```json
"ledger": 4635341,
"settings_last_modified": {
  "CONFIG_SETTING_CONTRACT_BANDWIDTH_V0": 606666,
  "CONFIG_SETTING_CONTRACT_COMPUTE_V0": 606666,
  "CONFIG_SETTING_CONTRACT_EVENTS_V0": 606666,
  "CONFIG_SETTING_CONTRACT_HISTORICAL_DATA_V0": 606666,
  "CONFIG_SETTING_CONTRACT_LEDGER_COST_V0": 3470630,
  "CONFIG_SETTING_STATE_ARCHIVAL": 2332
}
```

So `ledger` answers *"when was this snapshot taken?"* and
`settings_last_modified` answers *"when did each price last move?"* — two
different questions, now both answerable from one snapshot.

Snapshots written by earlier versions have no `settings_last_modified` key and
still load; the field simply reads as empty for them.

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
