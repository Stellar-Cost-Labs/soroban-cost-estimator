# Installation

## Prerequisites

- **Rust 1.85+** — the crate declares `rust-version = "1.85"` (edition 2024).
- **`wasm32v1-none` target** — only needed if you also want to *build*
  Soroban contracts (the fixture contract, for example). The estimator itself
  reads pre-compiled `.wasm` files and does not need the target.
- **Network access** to a Soroban RPC endpoint. The tool defaults to the
  well-known endpoints below; override any of them with `--rpc-url`.

| Network     | Endpoint                             |
|-------------|--------------------------------------|
| `testnet`   | `https://soroban-testnet.stellar.org` |
| `mainnet`   | `https://soroban.stellar.org`         |
| `futurenet` | `https://rpc-futurenet.stellar.org`   |

`--rpc-url` overrides network-based resolution for every command, so you can
point at a private or local RPC:

```bash
soroban-cost-estimator estimate \
  --wasm contract.wasm \
  --rpc-url https://custom-rpc.example.com
```

## From crates.io

```bash
cargo install soroban-cost-estimator
```

This installs the `soroban-cost-estimator` binary (and the developer-only
`gen_test_wasm` helper) into `~/.cargo/bin`.

## From source

```bash
git clone https://github.com/aigbagbobila/soroban-cost-estimator.git
cd soroban-cost-estimator
cargo install --path .
```

## Verify the install

```console
$ soroban-cost-estimator --help
Estimate Soroban contract costs & track network pricing changes

Usage: soroban-cost-estimator <COMMAND>

Commands:
  estimate      Simulate a single contract invocation and print the cost report
  estimate-all  Enumerate all public contract functions and estimate each one
  config        Fetch and store a snapshot of the network's resource-pricing configuration
  watch         Poll network config on an interval and print diffs when they appear
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

All commands write their state to `~/.soroban-cost-estimator/` — snapshots
under `snapshots/` and past estimate results under `cache/`. No database is
required. See [Caching](concepts/caching.md) for the cache layout.
