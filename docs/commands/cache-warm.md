# `config cache warm` / `cache warm`

Pre-populate the cache by estimating exported contract functions with
deterministic default arguments and writing results to the local cache. Both
`config cache warm` and `cache warm` are supported.

## Flags

```
Usage: soroban-cost-estimator config cache warm [OPTIONS] --wasm <WASM>
Usage: soroban-cost-estimator cache warm [OPTIONS] --wasm <WASM>

Options:
  -w, --wasm <WASM>        Path to the compiled Soroban contract `.wasm` file
      --network <NETWORK>  Network to simulate against [default: testnet]
      --rpc-url <RPC_URL>  Explicit RPC URL (overrides network-based resolution)
      --id <ID>            Deployed contract ID (64 hex chars) to invoke each function against
      --json               Output the warm-up summary as JSON
  -h, --help               Print help
```

## Behavior

- Simulates exported contract functions against the network, inferring
  deterministic defaults from contract-spec types (e.g. `false` for bool,
  `0` for numbers, `00` for bytes, `warm` for symbol/string).
- Each successful simulation is saved to the local cache, keyed by
  `wasm_hash + function_name + args_hash`.
- A progress bar is written to stderr during simulation. On completion, stdout
  reports `Warmed X functions into cache for <wasm_hash>` (or structured JSON with `--json`).
- Exits with 0 on completion.
- Useful for warming the cache before running `estimate` with `--cache-ttl`,
  so subsequent calls in CI or demo runs execute with zero latency from local cache.

## Examples

```bash
soroban-cost-estimator config cache warm \
  --wasm tests/fixtures/contract.wasm \
  --network testnet
```

With deployed contract ID:

```bash
soroban-cost-estimator config cache warm \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet
```
