# `config history`

Show the chronological change timeline for every config setting across all
stored snapshots for a network.

## Flags

```
Usage: soroban-cost-estimator config history [OPTIONS]

Options:
      --network <NETWORK>    Network whose snapshot history to inspect [default: testnet]
      --setting <SETTING>    Filter the timeline to a single setting (full field
                             path or any fragment of it)
      --json                 Output the timeline as a structured JSON array
  -h, --help                 Print help
```

## Behavior

- Loads all stored snapshots for the given network from
  `~/.soroban-cost-estimator/snapshots/`.
- Orders them chronologically (by timestamp, with the ledger as tiebreaker)
  and compares consecutive snapshots pairwise, aggregating every setting
  change into one timeline.
- Each timeline row shows the **Date**, **Ledger**, **Setting Field**, **Old
  Value**, **New Value**, and **Delta Percentage** for the change.
- `--setting` narrows the timeline to one setting. The filter matches the raw
  field path exactly or any fragment of it:
  - `--setting fee_rate_per_instructions_increment` →
    `contract_compute.fee_rate_per_instructions_increment`
  - `--setting contract_bandwidth` → every bandwidth field
- `--json` prints the timeline as a JSON array; each element carries
  `field_path`, `timestamp`, `ledger`, `old_value`, `new_value`,
  `is_pricing_change`, and `delta_percent` (`null` when the transition is not
  numeric, e.g. a setting appearing or disappearing).
- This is a local-only command — no network calls are made (it reads
  previously saved snapshots).

## Example

```bash
soroban-cost-estimator config history --network testnet
```

Sample output (columns simplified for readability):

```text
Config change history for testnet (2 change(s)):

  Date                  Ledger  Setting Field                                             Old Value  New Value  Delta %
  2026-08-01T00:00:00Z  3400000 Contract Compute V0 > Fee Rate Per Instructions Increment  5          7          +40.0%
  2026-08-03T00:00:00Z  3450000 Contract Ledger Cost V0 > Fee Write Ledger Entry           2500       5000       +100.0%
```

Filter to one setting:

```bash
soroban-cost-estimator config history --setting fee_rate_per_instructions_increment
```

Structured timeline:

```bash
soroban-cost-estimator config history --json
```

```json
[
  {
    "field_path": "contract_compute.fee_rate_per_instructions_increment",
    "timestamp": "2026-08-01T00:00:00Z",
    "ledger": 3400000,
    "old_value": "5",
    "new_value": "7",
    "is_pricing_change": true,
    "delta_percent": 40.0
  }
]
```
