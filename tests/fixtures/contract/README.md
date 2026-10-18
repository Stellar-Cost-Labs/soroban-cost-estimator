# Contract Fixture — Cross-Check Record

This directory contains a **real, deployable Soroban contract** used as the
structural test fixture for `soroban-cost-estimator`. It was also deployed to
testnet and used to prove the tool's invocation path end-to-end
(`estimate --fn --arg` vs the native Stellar CLI, CID-025).

## The contract

- **Source**: `src/lib.rs` — `increment(env, step: i64) -> i64`, reads a
  stored counter, adds `step`, writes it back. The storage write gives
  `simulateTransaction` a non-trivial footprint (rent / refundable fee
  portion) so fee breakdowns are realistic.
- **Build**: `./build.sh` (requires the `wasm32v1-none` rustup target;
  release build, ~4.7 KB — debug builds are ~3.7 MB and get rejected by
  testnet's size limits).
- **Fixture binary**: `../contract.wasm`, SHA-256
  `ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd`.
  This is the exact WASM deployed to testnet — the tool's cached estimate for
  the deployed contract carries the same `wasm_hash`.

## Live testnet deployment (2026-07-31)

| Field | Value |
|-------|-------|
| Network | testnet |
| Contract ID | `CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T` |
| Deploy tx | `d89a51f0…` ([stellar.expert](https://stellar.expert/explorer/testnet/tx/d89a51f0c0c1c7d9a0497a59ec17611605e4e50401e9deccf8df2fe8de2ab6ef)) |

## Cross-check numbers — this tool vs `stellar contract invoke --cost`

Call: `increment(step=5)` on testnet, ledger ~3,898,1xx.

| Metric | This tool | Native CLI | Divergence |
|--------|-----------|------------|------------|
| CPU instructions | 524,389 | 524,389 | **0%** (exact) |
| Total resource fee (stroops) | 18,999 | 18,999 (±1 across runs) | **≤0.011%** |
| Read / Write entries | 1 / 1 | 1 / 1 | 0 |
| Write bytes | 136 | 136 | 0 |
| Tx size (XDR bytes) | 156 | (same envelope) | — |

The ~20% margin `simulateTransaction` itself applies is a buffer convention;
this run landed at ≤0.011% (a 1–2 stroop run-to-run rent variance, not a
math error — an earlier run ~40–50 ledgers prior reported 19,001).

Native CLI `transaction_data` XDR (base64) from that run, for independent
re-verification:

```
AAAAAAAAAAEAAAAH6hS8qZjpjw3bM46OXO9uGfBzeKO3HotPiGjO3IV+Ts0AAAABAAAABgAAAAG5ZBMYkIphpuxef7KqlEtGRyYtc4OTo72EdfbbkRNmcAAAABQAAAABAAgAZQAAAAAAAACIAAAAAAAASjc=
```

(Tail layout: `instructions u32` → `read_bytes u32` → `write_bytes u32` →
`resource_fee i64` = the last 20 bytes.)

## Reproduction steps

```bash
# 1. Build the fixture
cd tests/fixtures/contract && ./build.sh && cd ../../..

# 2. Install the WASM on testnet (capture the printed wasm hash)
stellar contract install --network testnet --source test-key \
  --wasm tests/fixtures/contract.wasm

# 3. Create the contract instance (or reuse the ID below)
stellar contract create --network testnet --source test-key \
  --wasm-hash <wasm-hash> \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T

# 4. Estimate with this tool
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --fn increment --arg step=5 --json

# 5. Cross-check with the native CLI (simulate-only, nothing submitted)
stellar contract invoke --network testnet \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --source-account test-key --send=no --cost --very-verbose \
  -- increment --step 5
```

Compare step 5's `instructions` and `resource_fee` / `min_resource_fee` with
step 4's `cpu_instructions` and `fee.total_stroops`. They should land within
~20% (the margin `simulateTransaction` itself applies); this fixture
reproduces ≤0.011%.

## Reproducible Fixture Regeneration & Verification

To regenerate the synthetic WASM fixtures (`minimal.wasm` and `zeroarg.wasm`):

```bash
cargo run --bin gen_test_wasm
```

To verify that all committed fixtures (`minimal.wasm`, `zeroarg.wasm`, and `contract.wasm`) match their expected deterministic contents without overwriting them:

```bash
cargo run --bin gen_test_wasm -- --check
```

To rebuild or verify `contract.wasm` from source with deterministic path remapping:

```bash
# Regenerate ../contract.wasm
./tests/fixtures/contract/build.sh

# Verify ../contract.wasm without overwriting it
./tests/fixtures/contract/build.sh --check
```

