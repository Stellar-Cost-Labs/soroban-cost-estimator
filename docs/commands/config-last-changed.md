# `config last-changed`

Show when each config setting last changed, across all stored snapshots.

## Flags

```
Usage: soroban-cost-estimator config last-changed [OPTIONS]

Options:
      --network <NETWORK>  Network whose snapshot history to inspect [default: testnet]
  -h, --help               Print help
```

## Behavior

- Loads all stored snapshots for the given network and determines the last
  point at which each individual setting changed.
- Prints a per-setting timestamp and snapshot reference.
- Helpful for identifying which settings have been stable for a long time
  versus which changed recently.
- This is a local-only command — no network calls are made.

## Example

```bash
soroban-cost-estimator config last-changed --network testnet
```

Sample output:

```text
Last-changed timestamps for testnet:

  contract_compute:                  2026-08-01 (snapshot testnet-2026-08-01T...)
  contract_ledger_cost:              2026-08-01 (snapshot testnet-2026-08-01T...)
  contract_historical_data:          never changed
  contract_events:                   never changed
  contract_bandwidth:                never changed
  state_archival:                    never changed
```
