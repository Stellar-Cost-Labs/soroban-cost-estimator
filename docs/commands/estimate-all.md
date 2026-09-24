# `estimate-all`

Enumerate every public contract function and estimate each one.

## Flags

```
Usage: soroban-cost-estimator estimate-all [OPTIONS] --wasm <WASM>

Options:
  -w, --wasm <WASM>        Path to the compiled Soroban contract `.wasm` file
      --network <NETWORK>  Network to simulate against [default: testnet]
      --id <ID>            Deployed contract ID (64 hex chars) to invoke each function against
      --json               Output as JSON instead of a human-readable list
  -h, --help               Print help
```

## Behavior

- Enumerates the contract's public functions from the WASM, decoding **typed
  parameter lists** from the `contractspecv0` section when present.
- Estimates each **zero-argument** function and prints a `[i/N]` progress
  line before every simulation, so you can watch progress on contracts with
  many functions.
- Functions that require arguments are reported as
  `Skipped: needs --fn/--arg (N param(s))` — prompting you to specify them
  manually — rather than silently skipped.
- Without `--id`, simulations run against a zeroed contract ID and will
  almost certainly fail the "no cost data / no latest ledger" guard; the tool
  prints a note telling you to pass `--id` for real numbers.

## Example

```bash
soroban-cost-estimator estimate-all \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet
```

The fixture contract exports one function, `increment(step: i64)`, which
needs an argument — so it is reported as skipped. Actual output from a live
testnet run:

```text
Enumerated 1 function(s) in WASM:
  1. increment(step: i64)

Contract spec: present (typed params decoded from contractspecv0)
[1/1] increment
── Estimating 'increment' ── Skipped: needs --fn/--arg (1 param(s))
```

To estimate that function, use the single-invocation
[`estimate`](estimate.md) command with `--fn increment --arg step=5`.

## Example — JSON mode

```bash
soroban-cost-estimator estimate-all \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --json
```

`--json` emits a single JSON object with the contract identity, the
per-function results, and an aggregate summary. The `functions` array is
**deterministically sorted by `function_name`**, and the command exits
**non-zero if any function failed** — after printing the full JSON, so
automation always receives valid output on stdout.

### Schema

| Field | Type | Description |
|-------|------|-------------|
| `contract_wasm_hash` | string | SHA-256 of the contract WASM bytes (hex) |
| `network` | string | Network the simulations ran against |
| `functions` | array | Per-function results, sorted by `function_name` |
| `total_summary` | object | Aggregate counts and totals (see below) |

Each entry in `functions`:

| Field | Type | Description |
|-------|------|-------------|
| `function_name` | string | Exported function name |
| `status` | `"success"` \| `"failed"` | Outcome of the estimate |
| `resources` | object \| null | CPU, memory, and read/write bytes — `null` when failed |
| `fee_breakdown` | object \| null | Same shape as [`estimate --json`](estimate.md)'s `fee` — `null` when failed |
| `error_message` | string | Present **only** when `status` is `"failed"` |

`resources` on success:

```json
{
  "cpu_instructions": 524389,
  "memory_bytes": 0,
  "read_bytes": 0,
  "write_bytes": 136
}
```

`total_summary`:

| Field | Type | Description |
|-------|------|-------------|
| `total_functions` | number | Functions considered |
| `successful` | number | Functions that estimated successfully |
| `failed` | number | Functions that failed (including those needing arguments) |
| `total_cpu_instructions` | number | Sum of CPU instructions across successful functions |
| `total_memory_bytes` | number | Sum of memory bytes across successful functions |
| `total_fee_stroops` | number | Sum of total fees across successful functions (stroops) |

### Example output

The fixture contract exports one function, `increment(step: i64)`, which
needs an argument — so it is reported as `failed` with an explanatory
`error_message` (use [`estimate`](estimate.md) with `--fn`/`--arg` for it):

```json
{
  "contract_wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "network": "testnet",
  "functions": [
    {
      "function_name": "increment",
      "status": "failed",
      "resources": null,
      "fee_breakdown": null,
      "error_message": "needs --fn/--arg (1 param(s))"
    }
  ],
  "total_summary": {
    "total_functions": 1,
    "successful": 0,
    "failed": 1,
    "total_cpu_instructions": 0,
    "total_memory_bytes": 0,
    "total_fee_stroops": 0
  }
}
```
