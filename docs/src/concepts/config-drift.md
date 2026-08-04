# Config Drift

The fee numbers on this site are only as good as the network's
resource-pricing configuration. That configuration changes over time — and
when it does, every estimate you took before the change is stale.

## Why pricing changes

Stellar's Soroban resource pricing lives in six on-chain `ConfigSetting`
ledger entries, governed by **protocol upgrades** that validators vote on:

| Setting | What it prices |
|---------|----------------|
| `ConfigSettingContractComputeV0` | CPU instructions (rate per 10,000), memory limits |
| `ConfigSettingContractLedgerCostV0` | Ledger reads/writes, disk bytes, **rent** rates |
| `ConfigSettingContractHistoricalDataV0` | Historical data storage |
| `ConfigSettingContractEventsV0` | Contract events |
| `ConfigSettingContractBandwidthV0` | Transaction size |
| `ConfigSettingStateArchival` | Entry TTLs and archival/eviction policy |

A protocol vote can adjust any of these — a rent-rate change, a new
per-instruction fee, a different transaction size cap. The observed cadence
is roughly every 3–4 months: **Protocol 26 went live May 2026** and the
**Protocol 27 vote was July 2026**. That is fast enough that an estimate
taken "last quarter" can no longer be trusted.

## What `config snapshot` records

`config snapshot` fetches all six entries in one batched `getLedgerEntries`
call, decodes the XDR (`stellar-xdr` 27.x, matching Protocol 27/26
mainnet), timestamps it, and stores it as JSON:

```
~/.soroban-cost-estimator/snapshots/testnet-2026-08-04T07-15-38.487702259+00-00.json
```

Each snapshot is a first-class, versioned artifact. You keep one per network
and compare across time.

## What `config diff` does

`config diff` fetches the *current* configuration, compares it field by
field against the most recent snapshot (or one you name with `--against`),
and prints:

1. The snapshot pair being compared, with ledger numbers.
2. Every changed field: `💰` marks a **pricing** change (a rate that feeds
   the fee math), `📋` a non-pricing change (a cap, limit, or window size).
3. A cross-reference of the estimate cache, naming past estimates that were
   recorded at an earlier ledger and may now be stale.

The exit code is meaningful for scripts: **0** when nothing changed, **1**
when a pricing change was detected.

```
$ soroban-cost-estimator config diff --network testnet
Config diff: 2026-08-04T07:15:38.487702259+00:00 (ledger 3470630) → 2026-08-04T07:15:39.237129507+00:00 (ledger 3470630)
Network: testnet

✅ No changes detected.

  1 cached estimate(s) from earlier ledger(s) — may be stale:
    - (wasm upload) @ ledger 0 (current: 3470630)
```

If a protocol vote had moved a rate, the output instead shows the change.
The pricing-change fields the tool tracks are `fee_rate_per_instructions_increment`,
`fee_disk_read_ledger_entry`, `fee_write_ledger_entry`, `fee_disk_read1_kb`,
`rent_fee1_kb_soroban_state_size_low`/`_high`, `fee_tx_size1_kb`,
`fee_historical1_kb`, `fee_contract_events1_kb`, and the two rent-rate
denominators — a change to any of them means cached fee numbers are no longer
current.

## The workflow this enables

A drift alert on `config diff` is a release trigger, not a background
curiosity. The intended loop:

1. `config snapshot` after each deployment or on a schedule.
2. Run `config diff` in CI or cron after **every protocol vote** (Protocol 26
   live May 2026; Protocol 27 vote July 2026).
3. When it exits 1, re-estimate the affected contracts, refresh the cache,
   and update any downstream cost assumptions before they mislead users.

The [watch](commands/watch.md) command automates step 2: it polls on an
interval and prints a diff the moment the network's pricing model changes.
