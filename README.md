<p align="center">
  <img src="./assets/logo.png" alt="Soroban Cost Estimator Logo" width="500"/>
</p>

<p align="center">
  <a href="https://github.com/aigbagbobila/soroban-cost-estimator/actions/workflows/ci.yml">
    <img src="https://github.com/aigbagbobila/soroban-cost-estimator/actions/workflows/ci.yml/badge.svg" alt="CI"/>
  </a>
  <a href="https://crates.io/crates/soroban-cost-estimator">
    <img src="https://img.shields.io/crates/v/soroban-cost-estimator" alt="Crates.io"/>
  </a>
  <a href="LICENSE-MIT">
    <img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0"/>
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.85%2B-blue" alt="Rust 1.85+"/>
  </a>
  <a href="https://soroban-cost-estimator.gitbook.io/soroban-cost_estimator-docs">
    <img src="https://img.shields.io/badge/docs-GitBook-3884FF" alt="Docs"/>
  </a>
</p>

# Soroban Cost Estimator

[📚 Documentation](https://soroban-cost-estimator.gitbook.io/soroban-cost_estimator-docs) · [🔄 Migration Guide](docs/migration.md)

**Estimate Soroban contract resource costs & track network pricing changes over time.**


This CLI tool wraps Stellar's `simulateTransaction` RPC to report real resource
consumption (CPU instructions, memory, read/write entries/bytes, tx size, rent)
and the fee in stroops/XLM for your compiled Soroban contract `.wasm` files.

## 🎯 What makes this different

The [Stellar Resource Usage Report](https://github.com/57blocks/stellar-resource-usage-report)
is a real-time profiler: it instruments your JavaScript/TypeScript test code and
prints resource tables (CPU instructions, memory, ledger entry sizes) from
transactions executed against a local `stellar/quickstart` container. It answers
*"what did my contract consume while I ran it just now?"*

**This tool solves a different problem by a different mechanism.** It needs no
test harness and no local container — it works from your **compiled artifacts**
and gets real numbers from live `simulateTransaction` RPC simulation against
testnet/mainnet. And it does something no other Soroban cost tool does: it
tracks the *network's resource-pricing configuration*
(`ConfigSettingContractComputeV0`, `ConfigSettingContractLedgerCostV0`, etc.)
as a first-class, versioned artifact.

It snapshots that config, diffs it against a previous snapshot, and tells you
explicitly:

> *"The network's pricing model changed since your last estimate — here's what
> moved and what it does to your contract's cost."*

This makes it a **maintenance/monitoring tool**, not just a one-shot calculator.

> ⚠️ **Disclaimer:** This is unaudited developer tooling. Always verify fee
> estimates against your target network before mainnet deploy.

---

## Quick Start

```bash
# Build and install
git clone https://github.com/aigbagbobila/soroban-cost-estimator.git
cd soroban-cost-estimator
cargo install --path .

# 1. Estimate the cost of uploading a contract
soroban-cost-estimator estimate --wasm path/to/contract.wasm --network testnet

# 2. Save a config snapshot for later comparison
soroban-cost-estimator config snapshot --network testnet
# → Config snapshot saved to: ~/.soroban-cost-estimator/snapshots/testnet-<timestamp>.json

# 3. Check if the network's pricing model has changed (days/weeks later)
soroban-cost-estimator config diff --network testnet
# → 💰 fee_rate_per_instructions_increment: 5 → 7 (+40.0%)
# → 💰  1 cached estimate(s) may now be stale
```

## Commands

### `estimate`

Simulate a single contract invocation and print the cost report.

```bash
soroban-cost-estimator estimate \
    --wasm contract.wasm \
    --network testnet \
    [--id <contract-id-hex>] \
    [--fn my_function --arg key=val] \
    [--rpc-url https://custom-rpc.example.com] \
    [--json]
```

Without `--fn`, the tool simulates uploading the contract WASM to the network.
With `--fn`, it simulates invoking a specific contract function against a
**deployed** contract, so `--id <64-hex>` is required — `simulateTransaction`
loads the contract instance from the ledger, it cannot simulate against a
zeroed ID.

`--arg` values are type-inferred (`true`/`false` → bool, integers → i64/u64,
everything else → string), which is enough for cost estimation. Example:

```bash
soroban-cost-estimator estimate \
    --wasm contract.wasm \
    --id <contract-id-hex> \
    --fn increment \
    --arg step=5 \
    --network testnet
```

Use `--json` for machine-readable output (e.g., for CI pipelines).

The read/write entry counts and byte sizes in the report are decoded from the
simulation response's resource **footprint** — real values from the ledger
footprint, not zero-filled placeholders. If a fee-rate source
(`ConfigSetting*`) can't be fetched, the tool prints a warning naming the
source and zeroes only the affected rate (so the non-refundable fee is visibly
understated) rather than silently reporting a wrong fee.

### `estimate-all`

Enumerate every public contract function (including typed params decoded from
the contract's `contractspecv0` section) and estimate each zero-arg one.

```bash
soroban-cost-estimator estimate-all \
    --wasm contract.wasm \
    --id <contract-id-hex> \
    --network testnet \
    [--json]
```

Functions requiring arguments are reported as `"Skipped — needs --fn/--arg"`
(prompting you to specify them manually), rather than silently skipped.

A `[i/N] <function>` progress line is printed before each simulation, so you
can watch progress on contracts with many functions.

### `config snapshot`

Fetch all 6 `ConfigSetting` ledger entries, decode them via XDR, timestamp,
and save to disk.

```bash
soroban-cost-estimator config snapshot --network testnet [--out /custom/path.json] [--json]
```

Saved to `~/.soroban-cost-estimator/snapshots/<network>-<timestamp>.json`.
`--json` also prints the snapshot as JSON (and still saves it).

### `config diff`

Compare the current network config against the most recent (or explicit) snapshot.

```bash
soroban-cost-estimator config diff --network testnet [--against /path/to/snapshot.json]
```

- Exits **0** if no changes detected
- Exits **1** with a detailed field-by-field diff if pricing changed
- Cross-references the **cache** of past `estimate` results and reports which
  cached estimates are now stale due to the pricing change

### `watch`

Poll network config on an interval and print a diff whenever something changes.

```bash
soroban-cost-estimator watch --network testnet --interval 30m
```

Intervals accept `s`/`m`/`h`/`d` suffixes or bare seconds (default `1h`).
Useful in CI or cron jobs to monitor for unexpected pricing changes.

Press `Ctrl-C` (SIGINT) or send `SIGTERM` to stop **cleanly** (exit code 0):
the in-flight poll is cancelled rather than writing a partial snapshot.

## Installation

### Prerequisites

- **Rust 1.85+** (with `wasm32v1-none` target support for contract compilation)
- **Network access** to a Soroban RPC endpoint (testnet/mainnet/futurenet)

The tool uses these RPC endpoints by default:
| Network    | Endpoint                              |
|------------|---------------------------------------|
| `testnet`  | `https://soroban-testnet.stellar.org` |
| `mainnet`  | `https://soroban.stellar.org`         |
| `futurenet`| `https://rpc-futurenet.stellar.org`   |

Override with `--rpc-url` for custom endpoints.

### From source

```bash
cargo install --path .
```

Or install from [crates.io](https://crates.io/crates/soroban-cost-estimator):
```bash
cargo install soroban-cost-estimator
```

## How it works

```
┌─────────────┐     ┌──────────────┐     ┌────────────────┐
│  WASM File  │────▶│  Parse WASM  │────▶│  Enumerate fns │
└─────────────┘     └──────────────┘     └───────┬────────┘
                                                 │
                                                 ▼
┌─────────────┐     ┌──────────────┐     ┌────────────────┐
│  Testnet    │◀────│  RPC Client  │◀────│  Build TxEnv   │
│  RPC        │     └──────┬───────┘     └────────────────┘
└──────┬──────┘            │
       │                   ▼
       │            ┌────────────────┐     ┌────────────────┐
       ├───────────▶│ SimulateTx     │────▶│  Fee Breakdown │
       │            └────────────────┘     └────────────────┘
       │
       ▼
┌─────────────┐     ┌────────────────┐     ┌────────────────┐
│  Config     │────▶│  Diff & Report │     │  Cache Result  │
│  Settings   │     └────────────────┘     └────────────────┘
└─────────────┘
```

1. **WASM parsing**: Reads and validates your compiled `.wasm` file using
   `wasmparser`, enumerating exported functions and their parameter counts for
   multi-invocation estimation.
2. **RPC simulation**: Constructs a `TransactionEnvelope` with an
   `InvokeHostFunctionOp` and calls `simulateTransaction` on the target network.
3. **Fee breakdown**: Parses `minResourceFee` from the simulation response and
   derives the non-refundable and refundable portions **independently** from
   the network's own config-sourced rates (CPU, storage I/O, bandwidth), using
   integer stroops math — no floating point. The response's resource footprint
   provides the real read/write entry counts and byte sizes.
4. **Config snapshotting**: Fetches `ConfigSetting*` entries via
   `getLedgerEntries`, decodes the XDR using `stellar-xdr` 27.x (big-endian),
   and stores them as versioned JSON snapshots.
5. **Config drift detection**: Compares two snapshots field-by-field and reports
   which pricing parameters changed, flagging cached estimates that are now stale.

## Storage

All data is stored locally — no database required:

| Directory | Purpose |
|-----------|---------|
| `~/.soroban-cost-estimator/snapshots/` | Timestamped config snapshots (JSON) |
| `~/.soroban-cost-estimator/cache/` | Past `estimate` results, keyed by wasm hash + function + args hash |

The cache enables `config diff` to tell you *which* of your past estimates are
now stale after a network pricing change.

## ✅ Verified against live testnet

The invocation path is proven end-to-end against a **real deployed contract**
on Stellar testnet, cross-checked against the native Stellar CLI:

- **Deployed contract**: an `increment(step: i64)` Soroban contract — its wasm
  hash matches `tests/fixtures/contract.wasm` exactly.
- **Contract ID**: `CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T`
- **Cross-check**: the same invocation simulated by this tool vs
  `stellar contract invoke --cost`:

| Metric | This tool | Native CLI | Divergence |
|--------|-----------|------------|------------|
| CPU instructions | 524,389 | 524,389 | **exact match** |
| Total fee (stroops) | 18,999 | 18,999 | **≤ 0.011%** |

Full reproduction steps (`stellar contract install` → `create` →
`soroban-cost-estimator estimate --fn increment --arg step=5` →
`stellar contract invoke --cost`) and the complete record live in
[`tests/fixtures/contract/README.md`](tests/fixtures/contract/README.md).

## Project Status

| Feature | Status |
|---------|--------|
| WASM parsing + function enumeration | ✅ |
| `estimate` (single invocation) | ✅ |
| `estimate-all` (multi-function) | ✅ |
| `config snapshot` (6 config settings) | ✅ |
| `config diff` + stale cache detection | ✅ |
| `watch` (polling) | ✅ |
| JSON output (`--json`) | ✅ |
| Fee breakdown (non-refundable/refundable) | ✅ |
| Estimate result caching | ✅ |
| Verified against live testnet (cross-checked) | ✅ |
| Footprint read/write entries & bytes (real, not zeros) | ✅ |
| Watch graceful shutdown (SIGINT/SIGTERM) | ✅ |
| `estimate-all` progress indicator | ✅ |
| Fee-rate source degradation warnings | ✅ |

## Testing & CI

53 tests (unit + integration) cover the fee math — including the regression
for the exact input that used to produce a negative refundable fee — plus RPC
response parsing, XDR decoding, the cache, config diff, the WASM parser, and
CLI behavior.

```bash
cargo test --all
cargo clippy --all-targets --all-features   # pedantic-level denies are on
cargo fmt --check
```

Every push runs these gates on GitHub Actions (`.github/workflows/ci.yml`,
job `build`): format → clippy → build → fixture → tests. `main` is protected —
the `build` check must be green for changes to merge.

## Topics

`stellar`, `soroban`, `cli`, `developer-tooling`, `gas-estimation`

## Socials

- [Telegram](https://t.me/+O3iICQDcZEViM2Nk)
- [Discord](https://discord.gg/KSatPckM2)

## Contact

- GitHub issues: <https://github.com/aigbagbobila/soroban-cost-estimator/issues>
- Maintainer (GitHub): [@aigbagbobila](https://github.com/aigbagbobila)
- Security disclosures: see [SECURITY.md](SECURITY.md) (Telegram, the Stellar ecosystem norm)

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at
your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for details on coding standards, PR
process, and project structure.

Looking for something to work on? The
[issue backlog](https://github.com/aigbagbobila/soroban-cost-estimator/issues)
holds scoped issues with Summary / Acceptance Criteria / Tech Stack — good
first tasks for the Drips Stellar Wave contributor sprints.
