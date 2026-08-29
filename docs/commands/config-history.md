# `config history`

Show the full chronological change log across all stored snapshots for a
network.

## Flags

```
Usage: soroban-cost-estimator config history [OPTIONS]

Options:
      --network <NETWORK>  Network whose snapshot history to inspect [default: testnet]
  -h, --help               Print help
```

## Behavior

- Loads all stored snapshots for the given network from
  `~/.soroban-cost-estimator/snapshots/`.
- Orders them chronologically and prints a field-by-field change log showing
  what changed between consecutive snapshots.
- Useful for understanding the full history of network pricing changes over
  time — not just the most recent diff.
- This is a local-only command — no network calls are made (it reads
  previously saved snapshots).

## Example

```bash
soroban-cost-estimator config history --network testnet
```

Sample output:

```text
Config change history for testnet (3 snapshots):

Snapshot 1 → 2 (2026-08-01 → 2026-08-03, ledger 3400000 → 3450000):
  💰 contract_compute.fee_rate_per_instructions_increment: 5 → 7

Snapshot 2 → 3 (2026-08-03 → 2026-08-04, ledger 3450000 → 3470630):
  ✅ No changes detected.
```
