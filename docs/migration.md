# Migration Guide: Stellar CLI to Soroban Cost Estimator

If you are currently using `stellar contract invoke --cost` to estimate transaction fees for your Soroban smart contracts, this guide will help you transition to using the **Soroban Cost Estimator**.

## Why Migrate?

While the native Stellar CLI provides basic fee estimation, the Soroban Cost Estimator offers:
- Detailed breakdowns of CPU, Memory, and Storage costs.
- Offline estimations based on recent network ledger snapshots.
- Automated CI integration to prevent regressions.

## Command Comparison

### Old Way (Stellar CLI)
```bash
stellar contract invoke \
  --id C... \
  --source S... \
  --network testnet \
  --cost \
  -- hello \
  --to "world"
```

### New Way (Soroban Cost Estimator)
```bash
soroban-cost-estimator estimate \
  --contract C... \
  --network testnet \
  -- hello \
  --to "world"
```

## Key Differences
- **Output Format**: The cost estimator outputs structured JSON or a human-readable table instead of raw XDR resource limits.
- **Dry-Run Default**: The cost estimator **never** submits the transaction. It always uses simulation.
- **Source Account**: You do not need a funded `--source` account to estimate costs unless your contract logic explicitly requires authentication.

## Next Steps
Read the [Usage Guide](../README.md) for advanced options like custom gas limits and CI integration.
