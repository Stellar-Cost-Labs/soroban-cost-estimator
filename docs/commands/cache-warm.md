# `cache warm`

Pre-populate the cache by estimating every exported function — equivalent to
running [`estimate-all`](estimate-all.md) with the side effect of caching all
results.

## Flags

```
Usage: soroban-cost-estimator cache warm [OPTIONS] --wasm <WASM>

Options:
  -w, --wasm <WASM>        Path to the compiled Soroban contract `.wasm` file
      --network <NETWORK>  Network to simulate against [default: testnet]
      --id <ID>            Deployed contract ID (64 hex chars) to invoke each function against
      --json               Output as JSON instead of a human-readable list
  -h, --help               Print help
```

## Behavior

- Internally delegates to `estimate-all` — enumerates all exported functions
  and simulates each zero-argument one against the network.
- Each successful simulation is saved to the cache, keyed by
  `wasm_hash + function_name + args_hash`.
- Useful for warming the cache before running `estimate` with `--cache-ttl`,
  so the first call in a CI pipeline hits the cache instead of triggering a
  fresh simulation.
- The `--json` flag produces the same JSON output as `estimate-all`.

## Example

```bash
soroban-cost-estimator cache warm \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet
```
