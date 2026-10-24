# Migrating from Stellar CLI cost checks

This guide is for contributors who already use `stellar contract invoke --cost`
and want to add repeatable Soroban cost estimates and network configuration
drift checks to their development or CI workflow.

`soroban-cost-estimator` complements the Stellar CLI. Use the CLI to build,
install, deploy, and invoke contracts. Use the estimator to simulate uploads
and invocations, report resource use and fees, compare builds, and track the
network pricing configuration those fee estimates depend on.

## Feature comparison

| Task | Stellar CLI | Soroban Cost Estimator |
|---|---|---|
| Simulate an invocation and inspect costs | `stellar contract invoke ... --send=no --cost --very-verbose` | `soroban-cost-estimator estimate --wasm ... --id ... --fn ... --arg ...` |
| Estimate a WASM upload | The CLI's install/deploy commands perform those operations; they are not a saved pricing baseline. | `soroban-cost-estimator estimate --wasm contract.wasm` simulates uploading the WASM. |
| Estimate several contracts or functions | Run separate CLI commands or script a loop. | Repeat `--wasm`, use `--wasm-dir`, or use `estimate-all`; batch reports aggregate results. |
| Keep a history of network pricing inputs | No built-in versioned config snapshot and diff workflow. | `config snapshot` stores config snapshots; `config diff` compares them with current network config or each other. |
| Detect pricing drift in CI | Parse command output and maintain a baseline yourself. | `config diff` reports pricing changes and exits `1` by default when pricing changes are detected. |

Both tools simulate transactions to obtain invocation costs. Neither a CLI
invocation with `--send=no` nor an estimator run submits that invocation.
Installation and deployment commands can change network state, so do not treat
them as dry-run cost checks.

## Equivalent workflows

The examples use `testnet`, a built contract at `target/wasm32v1-none/release/contract.wasm`,
and a contract already deployed at `C...`. Replace the paths, ID, function,
and arguments with values from your project.

### Contract upload

The CLI can install a WASM or deploy a contract, which performs the network
operation. The estimator simulates the WASM upload and reports its estimated
resource costs without installing it:

```sh
# Native CLI: installs the WASM on testnet (a network operation)
stellar contract install \
  --wasm target/wasm32v1-none/release/contract.wasm \
  --network testnet \
  --source alice

# Estimator: simulate the upload for a cost report (no submission)
soroban-cost-estimator estimate \
  --wasm target/wasm32v1-none/release/contract.wasm \
  --network testnet
```

Without `--fn`, `estimate` performs the upload simulation. It does not install
the WASM or create a contract instance.

### Function invocation

The native CLI's `--cost` option reports simulation costs. The estimator also
reports CPU, memory, ledger footprint, transaction size, and a fee breakdown
based on the selected network's current resource pricing configuration.

```sh
# Native CLI: simulate only; do not submit the invocation
stellar contract invoke \
  --id C... \
  --source-account alice \
  --network testnet \
  --send=no --cost --very-verbose \
  -- increment --step 5

# Estimator: simulate the same function using the contract WASM/spec
soroban-cost-estimator estimate \
  --wasm target/wasm32v1-none/release/contract.wasm \
  --id C... \
  --network testnet \
  --fn increment --arg step=5
```

For the fixture's recorded testnet cross-check, both tools reported `524389`
CPU instructions and a resource fee of about `18999` stroops. The precise fee
can vary slightly between ledgers. The CLI exposes these values in its verbose
simulation details; the estimator presents them in its cost report:

```text
Stellar CLI (--very-verbose):   instructions: 524389   resource_fee: 18999
Soroban Cost Estimator:         CPU Instructions: 524389
                                Total: 18999 stroops (0.0018999 XLM)
```

This is a representative comparison from the checked-in fixture record, not a
promise that every invocation or later ledger has identical costs. See the
[fixture cross-check](../tests/fixtures/contract/README.md) for the recorded
run details.

The estimator needs the WASM file to read the contract spec and hash the build.
It needs `--id` for an invocation because simulation runs against a deployed
contract. Argument values use `KEY=VALUE` and are inferred from the contract
spec where available.

### Batch estimation

For a set of contracts, the native CLI can be called repeatedly from a script.
The estimator accepts multiple WASM files in one run and produces an aggregate
report. `estimate-all` enumerates public functions, estimating zero-argument
functions and identifying functions that need explicit arguments.

```sh
# Native CLI: one simulated invocation per contract; aggregate in your own script
for contract_id in "$CONTRACT_ID_A" "$CONTRACT_ID_B"; do
  stellar contract invoke --id "$contract_id" --source-account alice \
    --network testnet --send=no --cost --very-verbose \
    -- increment --step 5
done

# Estimator: simulate uploading each WASM and summarize the batch
soroban-cost-estimator estimate --wasm-dir target/wasm32v1-none/release \
  --network testnet --format json

# Or enumerate one contract's public functions
soroban-cost-estimator estimate-all --wasm contract.wasm \
  --id C... --network testnet --json
```

The native CLI loop prints a separate simulation result for each invocation;
the estimator upload batch emits a single report with one result per WASM and
aggregate totals. For example, JSON output includes an array of contract
results plus a batch summary:

```text
Stellar CLI loop:  [result for contract-a]  [result for contract-b]  ...
Estimator batch:   {"contracts":[...],"total_contracts":2,"succeeded":2,"failed":0}
```

Use repeated `--wasm contract-a.wasm --wasm contract-b.wasm` instead of
`--wasm-dir` when the contracts are in different directories. Batch `estimate`
accepts the same invocation options for each WASM; omit `--fn` to estimate
uploads. A function that requires arguments cannot be inferred by
`estimate-all`; use `estimate` with `--fn` and `--arg` for that function.

## Track pricing configuration drift

An estimate depends on both the contract build and the network's resource
pricing configuration. A cost result alone does not tell you whether those
pricing inputs changed between two runs. The estimator can save timestamped,
versioned configuration snapshots and compare them over time:

```sh
# Capture a baseline after reviewing or deploying
soroban-cost-estimator config snapshot --network testnet

# Later, compare live network pricing with the latest snapshot
soroban-cost-estimator config diff --network testnet
```

`config diff` reports changed settings and checks cached estimates made at an
earlier ledger that may now be stale. It automatically saves a snapshot when
it detects a pricing change, making that configuration the baseline for the
next diff. Old-ledger estimates are reported as potentially stale, not deleted
or silently treated as current; re-run the estimates you rely on after
reviewing a pricing change. A newer ledger alone can make an estimate appear
stale even if pricing did not change.

For CI, the default exit status is `0` when no pricing change is detected and
`1` when pricing changes are detected. Use `--fail-on-any-change` to fail for
non-pricing config changes too, or `--ignore-pricing-exit` to report drift
without failing the job:

```sh
# Fail CI when network pricing changes
soroban-cost-estimator config diff --network testnet

# Optional: fail on any setting change, including limits and caps
soroban-cost-estimator config diff --network testnet --fail-on-any-change
```

See [Config Drift](concepts/config-drift.md), [Caching](concepts/caching.md),
and the [`config diff` reference](commands/config-diff.md) for details.
