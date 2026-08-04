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

Skipped and successful functions are reported as JSON records:

```json
[
  {
    "function": "increment",
    "status": "skipped",
    "reason": "needs --fn/--arg (1 param(s))"
  }
]
```
