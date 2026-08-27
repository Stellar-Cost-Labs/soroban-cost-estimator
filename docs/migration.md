# Migrating from `stellar contract invoke --cost`

If you currently read cost numbers off `stellar contract invoke --cost`, this
page maps that workflow onto `soroban-cost-estimator`: what each flag becomes,
what the output fields correspond to, and which habits need to change.

## Why switch

`stellar contract invoke --cost` is part of the Stellar CLI. It builds and
**submits** (or, with `--is-view`/simulation, evaluates) an invocation and
prints the resource usage the transaction incurred. It answers *"what did this
one call cost, right now?"*.

`soroban-cost-estimator` never submits anything. It builds a simulation
envelope from your compiled `.wasm`, calls `simulateTransaction`, and reports
the same resources plus:

- a **fee breakdown** derived independently from the live network rate config
  (compute, ledger cost, bandwidth) rather than a single opaque total;
- **config-drift tracking** — snapshot the network's `ConfigSetting*` pricing
  entries, diff them later, and find out which of your recorded estimates the
  change made stale;
- **batch estimation** over every exported function (`estimate-all`);
- a stable **`--json` shape** for CI.

The numbers agree: on testnet, `increment(step=5)` reports identical CPU
instructions and a total resource fee within 0.011% of the native CLI. See
[Verification](verification.md) for the full cross-check.

## The one prerequisite difference

The Stellar CLI works from a deployed contract ID and, for signing, a
configured identity. This tool works from the **compiled artifact**:

- `--wasm <path>` is **always required**, including when you invoke a function.
  The WASM is what the tool hashes for the cache and what it uploads in the
  upload-simulation path.
- **No identity, no keys, no funded account.** Nothing is signed and nothing is
  submitted, so there is no `--source`/`--source-account` equivalent.
- `--id` is still needed to invoke a *deployed* function, because
  `simulateTransaction` loads the contract instance from the ledger.

## Command mapping

### Invoke one function

```bash
# Stellar CLI
stellar contract invoke \
  --id CC4WIEYY...NTHAQ5T \
  --network testnet \
  --source test-key \
  --cost \
  -- increment --step 5
```

```bash
# soroban-cost-estimator
soroban-cost-estimator estimate \
  --wasm target/wasm32v1-none/release/contract.wasm \
  --id CC4WIEYY...NTHAQ5T \
  --network testnet \
  --fn increment --arg step=5
```

### Cost of uploading a contract

The Stellar CLI reports this as part of `stellar contract install`/`deploy`.
Here it is just `estimate` with `--fn` omitted:

```bash
soroban-cost-estimator estimate --wasm contract.wasm --network testnet
# report's function column reads "(wasm upload)"
```

### Every function at once

No native equivalent — you would loop over invocations by hand:

```bash
soroban-cost-estimator estimate-all \
  --wasm contract.wasm --id CC4WIEYY...NTHAQ5T --network testnet
```

Functions that take parameters are **skipped** with a reason, since
`estimate-all` passes no arguments; estimate those individually with
`estimate --fn`.

### Track pricing changes over time

No native equivalent at all:

```bash
soroban-cost-estimator config snapshot --network testnet   # today
soroban-cost-estimator config diff --network testnet       # weeks later
```

`config diff` exits **1** when a pricing-relevant field moved, so CI can gate
on it. See [Config Drift](concepts/config-drift.md).

## Flag mapping

| Stellar CLI | This tool | Notes |
|---|---|---|
| `--id <CONTRACT_ID>` | `--id <CONTRACT_ID>` | Same value. Required with `--fn`. |
| `--network <NAME>` | `--network <NAME>` | `testnet`, `mainnet`, `futurenet`. |
| `--rpc-url <URL>` | `--rpc-url <URL>` | On `estimate`; other commands resolve from `--network`. |
| `--network-passphrase` | *(none)* | Nothing is signed, so no passphrase is needed. |
| `--source` / `--source-account` | *(none)* | No identity required. |
| `-- <fn> --arg val` | `--fn <fn> --arg key=val` | Args are `key=value`, before the function runs; no `--` separator. |
| `--cost` | *(always on)* | Cost reporting is the whole point; there is no flag to turn it off. |
| `--is-view` | *(always simulated)* | Every call is a simulation; nothing is ever submitted. |
| *(none)* | `--json` | Machine-readable report for CI. |
| *(none)* | `--wasm <PATH>` | Required — the compiled artifact is the input. |

### Argument syntax

The Stellar CLI takes contract arguments after a `--` separator, typed from the
contract spec. This tool takes repeated `--arg key=value` pairs and **infers**
the type: `true`/`false` → bool, integers → i64/u64, anything else → string.
That is enough for cost estimation, where the argument's size and type class
drive the resources, not its semantics.

```bash
# Stellar CLI:            -- increment --step 5 --flag true
# soroban-cost-estimator: --fn increment --arg step=5 --arg flag=true
```

## Output mapping

The Stellar CLI prints a cost block; this tool prints a report table (or JSON).
Fields line up as follows:

| Stellar CLI `--cost` | This tool | Notes |
|---|---|---|
| `cpu_insns` | CPU instructions | Exact match in the cross-check. |
| `mem_bytes` | Memory bytes | Only reported by legacy RPC versions; modern RPCs omit it and this tool reports `0`. |
| read/write ledger entries | Read entries / Write entries | Decoded from the real simulation footprint. |
| read/write bytes | Read bytes / Write bytes | Same source. |
| transaction size | Tx size | Raw XDR byte count, not the base64 length. |
| resource fee | Total fee (stroops / XLM) | Prefers the RPC's `minResourceFee`, falling back to `transactionData.resourceFee`. |
| *(not broken out)* | Non-refundable / refundable split | Derived independently from live `ConfigSetting*` rates. |

Two behavioral notes worth carrying over:

- If a `ConfigSetting*` rate source cannot be fetched, the tool prints a warning
  naming it and zeroes **only** that rate — the non-refundable fee is then
  visibly understated rather than silently wrong.
- A simulation that comes back with no cost data *and* no latest ledger fails
  loudly (bad `--id`, wrong network, or RPC schema drift) instead of printing
  an all-zero report.

## Scripting: from `--cost` scraping to `--json`

Instead of parsing the CLI's human-readable cost block:

```bash
soroban-cost-estimator estimate \
  --wasm contract.wasm --id CC4WIEYY... --fn increment --arg step=5 \
  --network testnet --json \
  | jq '.fee.total_stroops'
```

`estimate-all --json` emits one record per function, each with a
`status` of `ok`, `skipped`, or `error`, so a batch run never fails silently:

```bash
soroban-cost-estimator estimate-all --wasm contract.wasm --network testnet --json \
  | jq '[.[] | select(.status == "ok")] | map(.fee_stroops) | add'
```

## What you give up

Be explicit about it — this tool is not a drop-in replacement for the Stellar
CLI:

- **It cannot submit transactions.** Deploy, install, and real invocations
  still go through `stellar`.
- **It does not decode return values.** You get resources and fees, not the
  contract's output.
- **Argument types are inferred, not spec-checked.** For a real invocation
  where argument semantics matter, use the Stellar CLI.
- **Memory bytes are unavailable on modern RPCs**, which no longer report them.

Keep both: `stellar` to deploy and call, `soroban-cost-estimator` to estimate,
budget, and watch for pricing drift.

## See also

- [`estimate`](commands/estimate.md) · [`estimate-all`](commands/estimate-all.md)
- [`config snapshot`](commands/config-snapshot.md) · [`config diff`](commands/config-diff.md) · [`watch`](commands/watch.md)
- [Resource Fees](concepts/resource-fees.md) · [Config Drift](concepts/config-drift.md) · [Caching](concepts/caching.md)
- [Verification](verification.md) — the live cross-check against `stellar contract invoke --cost`
