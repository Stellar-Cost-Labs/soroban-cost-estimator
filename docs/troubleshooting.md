# Troubleshooting Guide

This guide covers common errors, their causes, and solutions when using the soroban-cost-estimator.

## Common Errors

### Error: Contract not found

**Cause**: The specified contract file path does not exist or is incorrect.
**Solution**: Verify the path to the Wasm file is correct and the file exists.

### Error: Out of gas

**Cause**: The transaction exceeded the provided gas limit.
**Solution**: Increase the gas limit for your transaction or optimize the contract to consume less gas.

### Error: Invalid arguments

**Cause**: The arguments provided to the contract invocation are invalid or mismatch the contract's interface.
**Solution**: Double check the types and order of the arguments against the contract's expected inputs.

### Error: Network timeout

**Cause**: The tool could not reach the Stellar RPC node.
**Solution**: Check your internet connection and verify that the RPC URL is correct and the node is up.
