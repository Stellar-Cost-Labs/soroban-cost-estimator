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

## Protocol Version & Encoding Matrix (#506)

| Fixture | Method | Compatibility Case |
| --- | --- | --- |
| `simulate_transaction_modern_p22.json` | `simulateTransaction` | Modern Protocol 21/22+ shape: numeric `latestLedger`, decimal string `minResourceFee`, and authoritative resources encoded in `transactionData` XDR without a top-level `cost` object. |
| `simulate_transaction_legacy_p20.json` | `simulateTransaction` | Legacy Protocol 20 shape: string `latestLedger`, string `cost.cpuInsns` / `cost.memBytes`, and 8-byte base64 XDR `minResourceFee`. |
| `simulate_transaction_numeric_cost.json` | `simulateTransaction` | Transitional RPC shape: numeric `cost.cpuInsns` / `cost.memBytes`, numeric `latestLedger`, and decimal string `minResourceFee` when `transactionData` is absent. |
| `get_health_healthy.json` | `getHealth` | Modern `getHealth` response with numeric ledger window fields (`latestLedger`, `oldestLedger`, `ledgerRetentionWindow`). |
| `get_health_legacy_strings.json` | `getHealth` | Legacy `getHealth` response where ledger window values are serialized as JSON strings. |
| `get_latest_ledger_p22.json` | `getLatestLedger` | Standard `getLatestLedger` response carrying `id`, `protocolVersion`, and `sequence`. |
| `get_ledger_entries_p22.json` | `getLedgerEntries` | Modern `getLedgerEntries` response with numeric `latestLedger` and optional `liveUntilLedgerSeq`. |
| `get_ledger_entries_legacy_p20.json` | `getLedgerEntries` | Legacy `getLedgerEntries` response where `latestLedger` arrives as a JSON string and `liveUntilLedgerSeq` is omitted. |
