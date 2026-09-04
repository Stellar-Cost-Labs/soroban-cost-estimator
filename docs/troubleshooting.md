# Troubleshooting Guide

This guide covers common errors, their causes, and solutions when using the soroban-cost-estimator.

## Common Errors by Category

### File & WASM Errors

#### Error: `File not found: <path>`
**Cause**: The specified WASM file path does not exist or is incorrect.
**Solution**: Verify the path to the `.wasm` file is correct and the file exists. Use an absolute path or ensure the relative path is from the current working directory.

#### Error: `WASM validation error: <details>`
**Cause**: The WASM file fails structural validation (malformed binary, invalid sections, etc.).
**Solution**: Rebuild the contract with `cargo build --target wasm32-unknown-unknown --release` and ensure the output `.wasm` file is not corrupted.

#### Error: `WASM parse error: <details>`
**Cause**: The WASM parser encountered an error reading the binary (e.g., truncated file, invalid LEB128 encoding).
**Solution**: Verify the WASM file is complete and not corrupted. Rebuild if necessary.

#### Error: `no exported functions found in WASM binary`
**Cause**: The WASM file has no exported functions, meaning it's not a valid Soroban contract or was built incorrectly.
**Solution**: Ensure the contract has `#[contractimpl]` functions and is built with the correct target. Check that `cargo build --target wasm32-unknown-unknown --release` produces a valid contract.

---

### RPC & Network Errors

#### Error: `RPC error (status <code>): <message>`
**Cause**: The Soroban RPC node returned an error response. Common codes:
- `-32600`: Invalid request (malformed JSON-RPC)
- `-32601`: Method not found (RPC version mismatch)
- `-32602`: Invalid params (bad arguments to `simulateTransaction`)
- `-32603`: Internal error (node issue)
- `-32000` to `-32099`: Server-defined errors (e.g., contract not found, out of gas)
**Solution**: Check the RPC error message for details. Verify the contract ID is correct, the network matches the contract deployment, and the function arguments match the contract's interface.

#### Error: `HTTP request failed: <details>`
**Cause**: Network-level failure connecting to the RPC endpoint (DNS resolution, connection timeout, TLS error, etc.).
**Solution**: 
- Verify internet connectivity
- Check the RPC URL is correct (use `--rpc-url` to override)
- Ensure the RPC node is operational (check status page)
- Try a different network (`testnet`, `mainnet`, `futurenet`)

#### Error: `RPC endpoint not configured for network: <network>`
**Cause**: An unknown network name was provided (not `testnet`, `mainnet`, or `futurenet`) and no custom `--rpc-url` was given.
**Solution**: Use a supported network name or provide a custom RPC URL with `--rpc-url https://your-rpc-endpoint`.

#### Error: `simulation returned no cost data and no latest ledger — check --id, --fn, and the RPC endpoint`
**Cause**: The `simulateTransaction` RPC call succeeded but returned no resource usage data. This typically happens when:
- The contract ID (`--id`) is wrong or not deployed on the target network
- The function name (`--fn`) doesn't exist on the contract
- The RPC endpoint is for a different network than where the contract is deployed
- The RPC schema has changed and the tool needs updating
**Solution**: 
- Verify the contract ID is correct (64 hex chars or `C...` strkey)
- Ensure the contract is deployed on the target network
- Check the function name matches exactly (case-sensitive)
- Try with `--rpc-url` pointing to a known-good endpoint

---

### Transaction Construction Errors

#### Error: `Transaction construction error: contract id required for function invocation (pass --id <64-hex>)`
**Cause**: A function name was provided via `--fn` but no contract ID was given via `--id`.
**Solution**: Provide the deployed contract ID with `--id <contract-id>`.

#### Error: `Transaction construction error: invalid contract id (expected 64 hex chars or a C… strkey): <details>`
**Cause**: The contract ID format is invalid. Must be either 64 hex characters or a `C...` strkey (SEP-23 format).
**Solution**: Use the contract ID from `stellar contract deploy` output or the Stellar dashboard.

#### Error: `Transaction construction error: ScSymbol: <details>`
**Cause**: The function name contains invalid characters for a Soroban symbol (must be valid UTF-8, max 32 bytes).
**Solution**: Ensure the function name matches the contract's exported function name exactly.

#### Error: `Transaction construction error: ScVal args: <details>`
**Cause**: Argument encoding failed (e.g., too many arguments, argument type mismatch).
**Solution**: Check the number and format of `--arg` values. The tool infers types: `true`/`false` → `Bool`, integers → `I64`/`U64`, everything else → `String`.

---

### XDR Errors

#### Error: `XDR decode error: base64 decode: <details>`
**Cause**: Failed to decode base64-encoded XDR data from the RPC response.
**Solution**: This usually indicates an RPC version mismatch or corrupted response. Try updating the tool or using a different RPC endpoint.

#### Error: `XDR decode error: LedgerEntryData from_xdr: <details>`
**Cause**: The XDR data from the RPC cannot be parsed as a `LedgerEntryData`.
**Solution**: RPC schema may have changed. Check for tool updates.

#### Error: `XDR decode error: expected ConfigSetting entry, got <type>`
**Cause**: The RPC returned a ledger entry that is not a `ConfigSetting` when fetching network configuration.
**Solution**: The network may not have the expected config settings. This can happen on older protocol versions.

#### Error: `XDR encode error: <details>`
**Cause**: Failed to encode a transaction envelope to XDR (should not happen in normal operation).
**Solution**: Report as a bug with the exact error message.

---

### Configuration & Snapshot Errors

#### Error: `Snapshot not found: <path>`
**Cause**: The specified config snapshot file does not exist.
**Solution**: 
- Run `soroban-cost-estimator config snapshot` first to create a baseline
- Check the snapshot directory (`~/.soroban-cost-estimator/snapshots/`)
- Use `--against` to specify an explicit snapshot path for `config diff`

#### Error: `Snapshot parse error: <details>`
**Cause**: The snapshot file exists but contains invalid JSON or missing required fields.
**Solution**: Delete the corrupted snapshot and run `config snapshot` again to regenerate.

#### Error: `No snapshots available for network: <network>`
**Cause**: No snapshots have been created for the specified network yet.
**Solution**: Run `soroban-cost-estimator config snapshot --network <network>` to create the first snapshot.

#### Error: `Config fetch error: <details>`
**Cause**: Failed to fetch config settings from the RPC node (network error or RPC error).
**Solution**: Check network connectivity and RPC endpoint. The node may not support `getLedgerEntries` for config settings.

#### Error: `Config setting not found: <setting>`
**Cause**: A specific config setting (e.g., `ContractComputeV0`) is not present on the network.
**Solution**: The network may be on an older protocol version. Some settings are only available after certain protocol upgrades.

---

### Simulation & Fee Calculation Errors

#### Error: `Simulation failed: <details>`
**Cause**: The `simulateTransaction` RPC call failed. The error message from the RPC is included.
**Common subcases**:
- "Contract not found" → Contract ID wrong or not deployed
- "Function not found" → Function name doesn't match
- "Invalid arguments" → Argument types don't match contract interface
- "Out of gas" → Transaction exceeds gas limits
**Solution**: Check the specific error message and verify contract ID, function name, and arguments.

#### Error: `Fee calculation error: <details>`
**Cause**: Internal error computing the fee breakdown from simulation results.
**Solution**: This is likely a bug. Report with the simulation output and error details.

---

### Cache Errors

#### Error: `could not determine home directory`
**Cause**: The tool cannot find the user's home directory to locate the cache/snapshot directories.
**Solution**: Ensure the `HOME` environment variable is set (Linux/macOS) or `USERPROFILE` (Windows).

#### Cache verification fails: `<filename>` reported as corrupt
**Cause**: A cache entry file is not valid JSON or is missing required fields.
**Solution**: Run `soroban-cost-estimator cache verify` to see all corrupt entries. Delete the corrupt files manually from `~/.soroban-cost-estimator/cache/` or clear the entire cache directory.

---

## Debugging Tips

### Enable Debug Logging
Set the `RUST_LOG` environment variable for verbose output:
```bash
RUST_LOG=debug soroban-cost-estimator estimate --wasm contract.wasm --network testnet
```

### Enable RPC Debugging
Log raw RPC requests and responses:
```bash
SCE_DEBUG_RPC=1 soroban-cost-estimator estimate --wasm contract.wasm --network testnet
```

### Common Workflow Issues

| Issue | Likely Cause | Fix |
|-------|--------------|-----|
| Estimates seem wrong | Network config drift | Run `config diff` to check for pricing changes |
| "Skipped" functions in `estimate-all` | Function requires arguments | Use `estimate` with `--fn` and `--arg` for specific functions |
| Fee shows 0 or very low | Fee rate config not fetched | Check warnings about "fee rate source unavailable" |
| Different results on re-run | Network state changed | Normal — ledger state affects simulation |

### Getting Help

1. Check existing issues: https://github.com/your-repo/soroban-cost-estimator/issues
2. Run with `RUST_LOG=debug` and include output when filing a bug
3. Include: command run, network, contract ID (if public), and full error message