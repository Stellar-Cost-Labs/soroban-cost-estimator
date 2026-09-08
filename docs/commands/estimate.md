# `estimate`

Simulate a single contract invocation and print the cost report.

## Flags

```
Usage: soroban-cost-estimator estimate [OPTIONS] --wasm <WASM>

Options:
  -w, --wasm <WASM>        Path to the compiled Soroban contract `.wasm` file
      --network <NETWORK>  Network to simulate against [default: testnet]
      --rpc-url <RPC_URL>  Explicit RPC URL (overrides network-based resolution)
      --fn <FN>            Contract function name to invoke
      --id <ID>            Deployed contract ID (64 hex chars) to invoke. Required when --fn is used
      --arg <KEY=VAL>      Function arguments as key=value pairs (value is type-inferred)
      --auto-snapshot      Automatically snapshot network config before estimating
      --json               Output as JSON instead of a human-readable table
  -h, --help               Print help
```

`--wasm` is required. Everything else is optional.

## Behavior

- **Without `--fn`**, the tool simulates *uploading* the contract WASM to the
  network. The report's function is `(wasm upload)`.
- **With `--fn`**, it simulates invoking a specific contract function against
  a **deployed** contract, so `--id <64-hex>` is required —
  `simulateTransaction` loads the contract instance from the ledger and
  cannot simulate against a zeroed ID.
- **`--arg` values are type-inferred**: `true`/`false` → bool, integers →
  i64/u64, everything else → string. Enough for cost estimation.
- The read/write entry counts and byte sizes are decoded from the simulation
  response's resource **footprint** — real values from the ledger footprint,
  not zero-filled placeholders.
- If a fee-rate source (`ConfigSetting*`) can't be fetched, a warning naming
  the source is printed and only the affected rate is zeroed, so the
  non-refundable fee is visibly understated rather than silently wrong.
- A simulation that returns no cost data and no latest ledger fails loudly
  with an error naming `--id`, `--fn`, and the RPC endpoint — it is treated
  as a misconfigured request, not a free transaction.
- **`--auto-snapshot`** fetches the network config and saves a snapshot
  *before* the estimate runs, so every estimate doubles as a drift-detection
  checkpoint. A snapshot failure is a warning, never fatal — the estimate
  proceeds regardless. Progress lines are suppressed in machine formats
  (`--json`, `--format csv|markdown`); warnings go to stderr.

## Example — snapshot before estimating

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --fn increment --arg step=5 --auto-snapshot
```

Before the report is printed, the tool takes and saves a config snapshot:

```text
auto-snapshot: taking config snapshot before estimate…
auto-snapshot: saved to /home/you/.soroban-cost-estimator/snapshots/testnet-<timestamp>.json (network: testnet, ledger: 3470630)
```

## Example — upload simulation

```bash
soroban-cost-estimator estimate --wasm tests/fixtures/contract.wasm --network testnet
```

## Example — invoke a function against a deployed contract

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --fn increment --arg step=5
```

This is the exact invocation cross-checked against the native Stellar CLI
(see [Verification](../verification.md)). Actual output from a live testnet run:

```text
Function: increment
Network: testnet (ledger 3961551)
WASM hash: ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd

+------------------+----------+---------------+
| Resource         | Consumed | Fee (stroops) |
+=============================================+
| CPU Instructions | 524389   |               |
|------------------+----------+---------------+
| Memory Bytes     | 0        |               |
|------------------+----------+---------------+
| Read Entries     | 1        |               |
|------------------+----------+---------------+
| Write Entries    | 1        |               |
|------------------+----------+---------------+
| Read Bytes       | 0        |               |
|------------------+----------+---------------+
| Write Bytes      | 136      |               |
|------------------+----------+---------------+
| Transaction Size | 156      |               |
+------------------+----------+---------------+

Fee Breakdown:
  Non-refundable: 4491 stroops
  Refundable:     12631 stroops
  Total:          17122 stroops (0.0017122)
```

The fee column in the resource table is intentionally blank: the per-resource
fee is embedded in the non-refundable total, which is derived from the
network's own config rates. See [Resource Fees](../concepts/resource-fees.md)
for the traced math.

## Example — machine-readable output

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --fn increment --arg step=5 --json
```

```json
{
  "function": "increment",
  "wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "cpu_instructions": 524389,
  "memory_bytes": 0,
  "tx_size": 156,
  "read_entries": 1,
  "write_entries": 1,
  "read_bytes": 0,
  "write_bytes": 136,
  "fee": {
    "non_refundable_stroops": 4491,
    "refundable_stroops": 12631,
    "total_stroops": 17122,
    "total_xlm": "0.0017122"
  },
  "ledger": 3961544,
  "network": "testnet"
}
```

Use `--json` when feeding the result into a CI pipeline or another tool. The
result is also written to the estimate cache(see [Caching](../concepts/caching.md)).
