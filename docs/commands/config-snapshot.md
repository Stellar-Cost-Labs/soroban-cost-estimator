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

# `config snapshot show`

Print a previously captured snapshot without touching the network. The
display groups the six settings into readable `Setting | Value` tables
(Compute, Ledger Cost, Historical Data, Events, Bandwidth, State Archival).

## Flags

```
Usage: soroban-cost-estimator config snapshot show [OPTIONS]

Options:
  [SNAPSHOT]           Explicit path to a snapshot JSON file
      --at <TIMESTAMP> Pick a stored snapshot by (prefix of its) timestamp
      --latest         Show the most recent snapshot (default when nothing else is given)
      --network <NETWORK>  Which snapshot history to read [default: testnet]
      --json           Print the raw snapshot JSON instead of the tables
  -h, --help           Print help
```

`[SNAPSHOT]`, `--at`, and `--latest` are mutually exclusive. Without any of
them the latest snapshot for the network is shown. `--at` accepts a prefix —
`--at 2026-08` resolves to the most recent stored snapshot from August 2026.

## Behavior

- `--latest` (explicit or implied) reads the newest snapshot for `--network`
  from `~/.soroban-cost-estimator/snapshots/`.
- `--at <PREFIX>` matches against the on-disk timestamp part of the filename;
  if several snapshots match, the most recent wins. No match is an error:
  `Snapshot not found for network ... at timestamp '<prefix>'` (exit 1).
- `[SNAPSHOT]` loads any snapshot file by path, regardless of directory.
- `--json` prints the snapshot exactly as stored — useful for scripting and
  for feeding `jq`.
- Snapshots captured before protocol-version tracking show
  `Protocol version: (unknown)` and sections that were not captured render as
  `(not captured)`.

## Examples

```bash
# Latest testnet snapshot as tables
soroban-cost-estimator config snapshot show

# The August 2026 testnet snapshot
soroban-cost-estimator config snapshot show --at 2026-08

# A specific file, JSON only
soroban-cost-estimator config snapshot show ./snap.json --json
```

Example output (truncated):

```text
Network:           testnet
Timestamp:         2026-08-04T07:15:38+00:00
Ledger:            3470630
Protocol version:  (unknown)
Tags:              (none)

Compute
┌────────────────────────────────────────┬─────────┐
│ Setting                                │ Value   │
├────────────────────────────────────────┼─────────┤
│ ledger_max_instructions                │ 100000000 │
│ fee_rate_per_instructions_increment    │ 20000   │
└────────────────────────────────────────┴─────────┘
State Archival
(not captured)
```
