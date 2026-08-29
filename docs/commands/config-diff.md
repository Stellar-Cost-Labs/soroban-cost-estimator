# `config diff`

Compare the network's current resource-pricing configuration against the most
recent snapshot.

## Flags

```
Usage: soroban-cost-estimator config diff [OPTIONS]

Options:
      --network <NETWORK>  Network to compare against [default: testnet]
      --against <AGAINST>  Explicit snapshot path to compare against (defaults to latest)
      --summary            Print a single-line count summary instead of the full diff
  -h, --help               Print help
```

## Behavior

- Fetches the current config, then compares it **field by field** against the
  latest snapshot for the network (or the snapshot at `--against`).
- `💰` marks a **pricing** change — a rate that feeds the fee math; `📋`
  marks a non-pricing change (a cap, limit, or window size).
- Always cross-references the estimate cache and reports cached estimates
  recorded at an earlier ledger as potentially stale.
- **Exit code 0** when nothing changed; **exit code 1** when a pricing change
  was detected — scripts and CI can branch on it.
- `--summary` prints a single line, `X pricing changes, Y non-pricing changes`
  (and suppresses the stale-cache and auto-save chatter), so you can read it
  directly into a CI status line. The exit code and auto-save side effects are
  unchanged. Example:

  ```bash
  soroban-cost-estimator config diff --network testnet --summary
  # 2 pricing changes, 1 non-pricing changes
  ```
- When a pricing change (a network protocol/config upgrade) is detected, the
  new config is **automatically saved** as a snapshot in
  `~/.soroban-cost-estimator/snapshots/`, so it becomes the baseline for the
  next diff. A failed save is reported as a warning and does not change the
  exit code.

## Example — nothing changed

```bash
soroban-cost-estimator config diff --network testnet
```

Actual output from a live testnet run (snapshot taken seconds earlier):

```text
Config diff: 2026-08-04T07:15:38.487702259+00:00 (ledger 3470630) → 2026-08-04T07:15:39.237129507+00:00 (ledger 3470630)
Network: testnet

✅ No changes detected.

  1 cached estimate(s) from earlier ledger(s) — may be stale:
    - (wasm upload) @ ledger 0 (current: 3470630)
```

Note the stale-cache cross-reference: this machine had one cached estimate
recorded at ledger 0, so the tool names it.See [Caching](../concepts/caching.md) for how that works.

## Example — comparing against an explicit snapshot

```bash
soroban-cost-estimator config diff --network testnet \
  --against ~/.soroban-cost-estimator/snapshots/testnet-2026-08-04T07-15-38.487702259+00-00.json
```

## What a pricing change looks like

If a protocol vote had moved a rate, the output would list the changed fields
instead of "No changes detected", for example:

```text
Config diff: 2026-07-31T13-14-29.307394561+00:00 (ledger 3470630) → 2026-08-04T07:15:39.237129507+00:00 (ledger 3470630)
Network: testnet

Found 1 field change(s):

  💰 contract_compute.fee_rate_per_instructions_increment
      Old: 5
      New: 7

⚠️  Pricing changes detected! Your cached estimates may be stale.
  Protocol upgrade detected — new config auto-saved to ~/.soroban-cost-estimator/snapshots/testnet-<timestamp>.json
```

and the command exits **1**. The post-upgrade config has already been saved,
so the next diff compares against it. Re-run `estimate` for the affected
contracts. See [Config Drift](../concepts/config-drift.md) for the workflow
this enables.
