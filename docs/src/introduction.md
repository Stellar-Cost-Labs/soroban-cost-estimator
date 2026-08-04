# Introduction

`soroban-cost-estimator` is a CLI tool that wraps Stellar's
`simulateTransaction` RPC to report the real resource consumption of your
compiled Soroban contract `.wasm` files — CPU instructions, memory,
read/write ledger entries and bytes, transaction size, and the resulting fee
in stroops and XLM. It works from compiled artifacts: no test harness, no
local container, no instrumentation of your test code. It simulates the
invocation against a live RPC endpoint (testnet, mainnet, or futurenet) and
prints a cost report, or a machine-readable JSON document for CI pipelines.

Every other Soroban cost tool tells you what your contract costs today; this
one tells you when your cost report is lying to you because the network
changed its prices. It snapshots the network's resource-pricing configuration
(the `ConfigSettingContract*` ledger entries) as a versioned artifact, diffs
it against previous snapshots, and cross-references its own cache of past
estimates to tell you exactly which ones are now stale after a pricing change.
That makes it a maintenance and monitoring tool, not just a one-shot
calculator.

## How this tool relates to other Soroban cost tools

The [Stellar Resource Usage Report](https://github.com/57blocks/stellar-resource-usage-report)
is a real-time profiler: it instruments your JavaScript/TypeScript test code
and prints resource tables (CPU instructions, memory, ledger entry sizes) from
transactions executed against a local `stellar/quickstart` container. It
answers *"what did my contract consume while I ran it just now?"*

This tool solves a different problem by a different mechanism. It needs no
test harness and no local container — it works from your compiled artifacts
and gets real numbers from live `simulateTransaction` RPC simulation against
testnet/mainnet. And it does something no other Soroban cost tool does: it
tracks the network's resource-pricing configuration as a first-class,
versioned artifact.

> ⚠️ **Disclaimer:** This is unaudited developer tooling. Always verify fee
> estimates against your target network before mainnet deploy.

## Project layout

| Directory | Purpose |
|-----------|---------|
| `src/` | CLI (`cli.rs`), WASM parsing (`wasm/`), RPC client (`rpc/`), fee math (`report/`), config snapshots (`config_snapshot/`), estimate cache (`cache.rs`) |
| `tests/` | Integration tests plus the fixture contract and its cross-check record (`fixtures/contract/README.md`) |

The full source is on GitHub: <https://github.com/aigbagbobila/soroban-cost-estimator>
