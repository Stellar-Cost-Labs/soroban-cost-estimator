# Stellar RPC Protocol Compatibility Fixtures

Sanitized JSON-RPC `result` payloads covering supported Stellar / Soroban RPC
protocol versions and field-encoding variations. All fixtures are deterministic,
offline, and free of endpoint credentials or private account/transaction data.

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
