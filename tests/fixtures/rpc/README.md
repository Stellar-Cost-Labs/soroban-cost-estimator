# RPC Protocol Response Fixtures

This directory contains sanitized, checked-in JSON-RPC response fixtures representing supported Stellar Soroban RPC protocol shapes. These fixtures ensure deterministic, offline verification of deserialization compatibility across current and legacy RPC versions.

## Fixtures & Compatibility Cases

### `simulateTransaction`
- **`simulate_transaction_current.json`**:
  - Represents modern Soroban RPC responses (Protocol 21+).
  - Numeric `latestLedger` sequence.
  - Plain decimal string `minResourceFee` (stroops).
  - Authoritative resource consumption (instructions, footprint, read/write bytes) and resource fee encoded in `transactionData` XDR.
  - Legacy `cost` object is absent.
- **`simulate_transaction_legacy.json`**:
  - Represents legacy Soroban RPC responses (Protocol 20).
  - String-formatted `latestLedger` sequence (`"3894195"`).
  - Base64-encoded XDR int64 `minResourceFee` (`"AAAAAAA81a8="`).
  - Resource usage reported in legacy `cost` object (`cpuInsns` and `memBytes` as strings).
  - `transactionData` is null.
- **`simulate_transaction_error.json`**:
  - Represents a contract execution failure / host error returned by simulation.
  - Carries an `error` description string and `latestLedger`.

### `getHealth`
- **`get_health_current.json`**:
  - Modern RPC health response including node identity metadata (`networkPassphrase`, `coreVersion`) and `latestLedger`.
- **`get_health_legacy.json`**:
  - Legacy minimal health response with only `status` and `latestLedger`.
- **`get_health_unhealthy.json`**:
  - Node health response reporting `unhealthy` status.

### `getLatestLedger`
- **`get_latest_ledger_current.json`**:
  - Modern ledger response with `sequence` and `protocolVersion` 22.
- **`get_latest_ledger_legacy.json`**:
  - Protocol 20 ledger response.

### `getLedgerEntries`
- **`get_ledger_entries_current.json`**:
  - Modern response with entry keys, XDR, `lastModifiedLedgerSeq`, and `liveUntilLedgerSeq`, plus numeric `latestLedger`.
- **`get_ledger_entries_legacy.json`**:
  - Legacy response with string `latestLedger` and without `liveUntilLedgerSeq`.

