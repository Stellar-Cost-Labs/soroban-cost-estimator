# Troubleshooting Guide

This guide documents common RPC, WASM, and network errors encountered when running `soroban-cost-estimator`, their root causes, and step-by-step solutions.

---

## 1. Common Errors & Solutions

### 1.1 `HostError (Error(Contract, #X))` — Contract Panic During Simulation

**Symptom:**
```text
Error: simulation error: HostError: Error(Contract, #1)
```
or
```text
Error: simulation error: HostError: Error(WasmVm, InvalidAction)
```

**Root Cause:**
The Soroban VM trapped while executing your contract function during `simulateTransaction`. Typical causes include:
- Calling a function before the contract's `initialize` / `__constructor` has been invoked on-chain.
- Passing arguments that violate a contract assertion or `panic_with_error!`.
- Reading a persistent/instance storage key that does not exist yet on the deployed contract (`--id`).

**Resolution:**
1. Verify that the deployed contract (`--id`) on `--network` has been initialized.
2. Check the contract's `#[contracterror]` enum definition to map `#X` to its human-readable variant name.
3. Inspect the expected parameter types with `wasm info`:
   ```bash
   soroban-cost-estimator wasm info --wasm path/to/contract.wasm
   ```
4. Supply valid arguments matching the expected types:
   ```bash
   soroban-cost-estimator estimate \
     --wasm path/to/contract.wasm \
     --id <64-char-hex-contract-id> \
     --fn transfer \
     --arg from=G... --arg to=G... --arg amount=100 \
     --network testnet
   ```

---

### 1.2 `TransactionSimulationFailed` — Footprint, Contract ID, or Auth Missing

**Symptom:**
```text
Error: simulation error: TransactionSimulationFailed: contract instance not found
```
or
```text
Error: --id <CONTRACT_ID> is required when --fn is specified
```

**Root Cause:**
- You specified `--fn <function_name>` without `--id`, or passed a contract ID (`C...` / 64-char hex) that is not deployed on the selected `--network` (e.g., passing a Testnet contract ID while targeting `--network mainnet`).
- Or the contract's ledger entry TTL has expired (archived state) and requires restoration before simulation can succeed.

**Resolution:**
1. Confirm the contract is deployed on the target `--network` and its 64-character hex contract ID is valid.
2. If you only want to estimate the cost of **uploading** the WASM bytecode (before deployment), omit `--fn` and `--id`:
   ```bash
   soroban-cost-estimator estimate --wasm path/to/contract.wasm --network testnet
   ```
3. If the contract entry is archived on-chain, restore it using `stellar contract restore` before simulating invocations.

---

### 1.3 `ConfigSettingNotFound` — Protocol Version Mismatch or Non-Soroban Node

**Symptom:**
```text
Error: config setting not found on ledger: CONFIG_SETTING_CONTRACT_COMPUTE_V0
```
or
```text
Warning: fee rate source(s) ContractComputeV0 unavailable — affected rate(s) set to 0
```

**Root Cause:**
The RPC endpoint returned an empty `entries` array for one or more Soroban `ConfigSettingId` ledger keys. This happens when:
- The RPC node is still syncing or has not upgraded to Soroban (Protocol 20+).
- A custom `--rpc-url` points to a Horizon REST endpoint instead of a Soroban JSON-RPC endpoint.

**Resolution:**
1. Verify the node's health and protocol version:
   ```bash
   curl -s -X POST https://soroban-testnet.stellar.org \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}'
   ```
2. Ensure `--rpc-url` points to a Soroban RPC endpoint (not Horizon).

---

### 1.4 `429 Too Many Requests` / Connection Timeouts — RPC Rate Limiting

**Symptom:**
```text
Error: RPC error (-32005): 429 Too Many Requests
```
or
```text
Error: HTTP request timed out after 30s
```

**Root Cause:**
Public RPC endpoints enforce rate limits per IP address, which can be triggered during large `estimate-all` batch runs or frequent `watch` polling.

**Resolution:**
1. Increase retry resilience and client-side rate limiting:
   ```bash
   soroban-cost-estimator estimate-all \
     --wasm contract.wasm \
     --id <contract-id> \
     --network testnet \
     --max-retries 5 \
     --timeout 60
   ```
2. Configure a commercial RPC provider with `--rpc-url` and authentication headers (`-H`), plus a `--fallback-rpc-url`:
   ```bash
   soroban-cost-estimator estimate \
     --wasm contract.wasm \
     --network mainnet \
     --rpc-url https://mainnet.provider.example.com \
     --fallback-rpc-url https://backup.provider.example.com \
     -H "Authorization=Bearer $RPC_TOKEN"
   ```

---

### 1.5 `InvalidSpec` / Missing `contractspecv0` Section

**Symptom:**
```text
Error: invalid WASM binary: malformed custom section 'contractspecv0'
```
or `wasm info` reports `Has Contract Spec: false`.

**Root Cause:**
- The `.wasm` file was compiled without the `soroban-sdk` contract macro (`#[contractimpl]`), or a custom post-processing tool (`wasm-strip` / aggressive `wasm-opt`) stripped the `contractspecv0` custom section.

**Resolution:**
1. Rebuild the contract using `stellar contract build` or `cargo build --target wasm32-unknown-unknown --release`.
2. If optimizing with `stellar contract optimize` or `wasm-opt`, ensure custom section `contractspecv0` is preserved.

---

## 2. Debugging with `--verbose` and `RUST_LOG`

`soroban-cost-estimator` uses `tracing` for structured diagnostic logging to `stderr` (never polluting `--json` output on `stdout`).

### Enable Verbose Output (`--verbose` / `-v`)
```bash
soroban-cost-estimator --verbose estimate --wasm contract.wasm --network testnet
```

### Enable Fine-Grained `RUST_LOG` Filtering
```bash
# Log all crate spans and RPC payloads at DEBUG level
RUST_LOG=soroban_cost_estimator=debug soroban-cost-estimator estimate \
  --wasm contract.wasm \
  --network testnet

# Trace low-level XDR encoding and HTTP retries
RUST_LOG=soroban_cost_estimator::rpc=trace,soroban_cost_estimator::wasm=debug \
  soroban-cost-estimator config snapshot --network testnet
```

> **Security Note:** Sensitive HTTP headers (`Authorization`, `X-API-Key`, `API-Key`, `Cookie`, `Proxy-Authorization`) are automatically masked as `<redacted>` in all verbose and debug logs.

---

## 3. Frequently Asked Questions (FAQ)

### Q: Why does `estimate` require `--id` when `--fn` is passed?
`simulateTransaction` executes your contract's function against the live ledger state (reading contract instance storage and code). Without a deployed contract ID (`--id`), the RPC node has no on-chain contract instance to load. To estimate contract **upload** costs before deployment, run `estimate --wasm <file>` without `--fn`.

### Q: Why are my cached estimates marked as stale after `config diff`?
When on-chain `CONFIG_SETTING` fee rates change on the network, any cached simulation fee calculated under the old rates no longer reflects live network pricing. Clear or refresh the cache with:
```bash
soroban-cost-estimator estimate --wasm contract.wasm --network testnet --clear-cache
```

### Q: Where are snapshots and cached estimates stored on disk?
By default, all data is stored under `~/.soroban-cost-estimator/`:
- `~/.soroban-cost-estimator/snapshots/<network>-<timestamp>.json`
- `~/.soroban-cost-estimator/cache/estimates.db`