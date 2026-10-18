# Migration Guide: From Stellar CLI (`stellar contract invoke --cost`) to `soroban-cost-estimator`

This guide helps developers familiar with `stellar-cli` (`stellar contract invoke --cost`, `stellar contract upload`, `stellar contract bindings`) adopt `soroban-cost-estimator` for contract fee profiling, CI regression gates, and on-chain resource-pricing drift monitoring.

---

## 1. Feature Comparison (`stellar-cli` vs. `soroban-cost-estimator`)

| Capability | `stellar contract invoke --cost` (`stellar-cli`) | `soroban-cost-estimator` |
|---|---|---|
| **Primary Purpose** | General-purpose contract deployment, invocation, and key management | Dedicated Soroban resource & fee profiler + network pricing drift monitor |
| **Execution Mode** | Submits live transactions by default unless `--send=no` / `--cost` is set | Always 100% dry-run simulation (`simulateTransaction` RPC); never submits or spends XLM |
| **Source Account Requirement** | Requires a funded `--source-account` keypair or identity | Uses a deterministic dummy source account automatically (no secret key required) |
| **Fee Breakdown** | Prints raw aggregate resource counters | Splits **Non-Refundable** (CPU, read/write entries, read bytes, tx bandwidth) vs. **Refundable** (rent & events) stroops using live `CONFIG_SETTING` rates |
| **Batch Function Profiling** | Manual per-function shell scripting | Built-in `estimate-all` auto-discovers exports from `contractspecv0` and profiles all zero-arg functions with summary statistics |
| **WASM-to-WASM Diffing** | Not built-in | Built-in `estimate --wasm v1.wasm --wasm-new v2.wasm --diff` side-by-side delta & percentage report |
| **Network Config Drift Tracking** | Not supported | Versioned `config snapshot`, `config diff`, `config history`, and `watch` commands for all 6 `CONFIG_SETTING` ledger entries |
| **Cache & Staleness Invalidation** | None | Content-addressed simulation cache (`~/.soroban-cost-estimator/cache/`) with config-drift staleness warnings and `--cache-ttl` |
| **Output Formats** | Text / raw XDR | Table, JSON (`--json`), CSV (`--format csv`), Markdown (`--format markdown`), HTML (`--format html`), and Flamegraph |

---

## 2. Side-by-Side Command Equivalents

### 2.1 Estimating Contract WASM Upload Cost

**With `stellar-cli`:**
```bash
stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/my_contract.wasm \
  --source-account alice \
  --network testnet \
  --send=no
```

**With `soroban-cost-estimator` (no funded account or identity needed):**
```bash
soroban-cost-estimator estimate \
  --wasm target/wasm32-unknown-unknown/release/my_contract.wasm \
  --network testnet
```

### 2.2 Estimating a Single Contract Function Invocation

**With `stellar-cli`:**
```bash
stellar contract invoke \
  --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
  --source-account alice \
  --network testnet \
  --cost \
  -- \
  increment \
  --step 5
```

**With `soroban-cost-estimator`:**
```bash
soroban-cost-estimator estimate \
  --wasm target/wasm32-unknown-unknown/release/my_contract.wasm \
  --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
  --fn increment \
  --arg step=5 \
  --network testnet
```

### 2.3 Batch-Estimating All Exported Contract Functions

**With `stellar-cli`:**
Requires manually inspecting the contract spec (`stellar contract info interface --wasm ...`) and writing a loop over each function name.

**With `soroban-cost-estimator`:**
```bash
soroban-cost-estimator estimate-all \
  --wasm target/wasm32-unknown-unknown/release/my_contract.wasm \
  --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
  --network testnet
```

### 2.4 Inspecting WASM Binary Structure & Debug Symbols

**With `stellar-cli`:**
```bash
stellar contract info interface --wasm contract.wasm
```

**With `soroban-cost-estimator`:**
```bash
soroban-cost-estimator wasm info --wasm contract.wasm
```
Reports total binary size, section-by-section byte share, exported functions with decoded `contractspecv0` parameter types, and whether unstripped DWARF/name debug sections are inflating your upload fee.

---

## 3. Unique Capabilities Beyond `stellar-cli`

### 3.1 Versioned Network Config Snapshots (`config snapshot`)

Soroban transaction fees are governed by six on-chain `CONFIG_SETTING` ledger entries (`ContractComputeV0`, `ContractLedgerCostV0`, `ContractHistoricalDataV0`, `ContractEventsV0`, `ContractBandwidthV0`, and `StateArchival`). Even if your `.wasm` code never changes, a network governance vote can change your contract's execution cost.

Capture a baseline snapshot of the network's fee configuration:

```bash
soroban-cost-estimator config snapshot --network testnet
# → Config snapshot saved to: ~/.soroban-cost-estimator/snapshots/testnet-2026-03-01T12-00-00Z.json
```

### 3.2 Network Pricing Drift Detection & CI Exit Codes (`config diff`)

Compare the network's latest configuration against your previous snapshot:

```bash
soroban-cost-estimator config diff --network testnet
```

- Exits with code `0` when no pricing parameters changed.
- Exits with code `1` when pricing parameters changed (or exceed `--threshold <PERCENT>`), making it a drop-in gate for scheduled CI workflows.

### 3.3 Automatic Cache Staleness Detection

Whenever `soroban-cost-estimator` caches a simulation result, it records the ledger sequence and active pricing rates. When `config diff` detects a network fee change, it automatically flags how many cached estimates were recorded under outdated fee rates and suggests refreshing them (`--clear-cache` or `cache warm`).

### 3.4 Quantified Optimization Recommendations

Every `estimate` report analyzes the simulation footprint against the network's live fee rates and prints actionable savings estimates (e.g., *"Removing one write entry saves ~2,500 stroops"* or warning when unoptimized `.debug*` sections inflate WASM upload size). Pass `--quiet` in scripts when you only want raw metrics.
