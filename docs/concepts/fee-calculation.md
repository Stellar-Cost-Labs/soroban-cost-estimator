# Soroban Fee Calculation Methodology

This document details the exact integer-arithmetic formulas and on-chain `stellar-xdr` configuration structures used by `soroban-cost-estimator` to calculate Soroban smart contract resource fees in **stroops** ($1\text{ stroop} = 10^{-7}\text{ XLM}$).

---

## 1. Two-Tier Fee Architecture

Every Soroban transaction fee is composed of a **Base Inclusion Fee** plus a **Soroban Resource Fee**:

$$\text{Total Fee} = \text{Base Fee} + \text{Resource Fee}$$

The **Resource Fee** itself is partitioned into two tiers:

1. **Non-Refundable Resource Fee ($\text{Fee}_{\text{non-ref}}$)**: Charged unconditionally for resources consumed during transaction execution and ledger validation (CPU instructions, ledger entry reads/writes, disk read bytes, and transaction bandwidth/historical size).
2. **Refundable Resource Fee ($\text{Fee}_{\text{ref}}$)**: Held as an up-front deposit for contract events and ledger state rent (TTL extensions). Unused portions are refunded to the fee-source account after execution.

$$\text{Resource Fee} = \text{Fee}_{\text{non-ref}} + \text{Fee}_{\text{ref}}$$

---

## 2. On-Chain `stellar-xdr` Configuration Sources

Rather than relying on hardcoded constants, `soroban-cost-estimator` fetches the active network pricing parameters via `getLedgerEntries` using the following `stellar_xdr::ConfigSettingEntry` structures:

| `ConfigSettingId` | `stellar-xdr` Struct | Pricing Fields Used |
|---|---|---|
| `ContractComputeV0` (`0`) | `ConfigSettingContractComputeV0` | `fee_rate_per_instructions_increment` (stroops per $10{,}000$ instructions) |
| `ContractLedgerCostV0` (`1`) | `ConfigSettingContractLedgerCostV0` | `fee_disk_read_ledger_entry`, `fee_write_ledger_entry`, `fee_disk_read1_kb`, `rent_fee1_kb_soroban_state_size_low`, `rent_fee1_kb_soroban_state_size_high`, `soroban_state_rent_fee_growth_factor` |
| `ContractHistoricalDataV0` (`2`) | `ConfigSettingContractHistoricalDataV0` | `fee_historical1_kb` (stroops per $1{,}024$ bytes of ledger archive history) |
| `ContractEventsV0` (`3`) | `ConfigSettingContractEventsV0` | `fee_contract_events1_kb` (stroops per $1{,}024$ bytes of emitted XDR events) |
| `ContractBandwidthV0` (`4`) | `ConfigSettingContractBandwidthV0` | `fee_tx_size1_kb` (stroops per $1{,}024$ bytes of transaction envelope XDR) |
| `StateArchival` (`5`) | `StateArchivalSettings` | `persistent_rent_rate_denominator`, `temp_rent_rate_denominator`, `min_persistent_ttl`, `min_temporary_ttl` |

---

## 3. Component Fee Formulas

All calculations in `src/report/fee_calc.rs` use 128-bit intermediate integer arithmetic (`i128`) with saturating bounds (`i64`) so that `(units * rate) / scale` never overflows or suffers from IEEE-754 floating-point rounding drift.

### 3.1 CPU Instruction Cost Scaling

From `ConfigSettingContractComputeV0.fee_rate_per_instructions_increment` ($R_{\text{cpu}}$), priced per $10{,}000$ instructions:

$$\text{Fee}_{\text{cpu}} = \left\lfloor \frac{N_{\text{insns}} \times R_{\text{cpu}}}{10{,}000} \right\rfloor$$

### 3.2 Storage Entry Read & Write Fees

From `ConfigSettingContractLedgerCostV0.fee_disk_read_ledger_entry` ($R_{\text{read\_entry}}$) and `fee_write_ledger_entry` ($R_{\text{write\_entry}}$):

$$\text{Fee}_{\text{read\_entries}} = N_{\text{read\_entries}} \times R_{\text{read\_entry}}$$

$$\text{Fee}_{\text{write\_entries}} = N_{\text{write\_entries}} \times R_{\text{write\_entry}}$$

Note: Per the Stellar protocol specification, every written ledger entry is also counted in the read footprint ($N_{\text{read\_entries}} \ge N_{\text{write\_entries}}$).

### 3.3 Storage Byte Read & Write Fees

From `ConfigSettingContractLedgerCostV0.fee_disk_read1_kb` ($R_{\text{read\_kb}}$), priced per $1\text{ KiB}$ ($1{,}024\text{ bytes}$):

$$\text{Fee}_{\text{read\_bytes}} = \left\lfloor \frac{B_{\text{read}} \times R_{\text{read\_kb}}}{1{,}024} \right\rfloor$$

Total non-refundable storage I/O fee is the sum of entry and byte fees:

$$\text{Fee}_{\text{storage}} = \text{Fee}_{\text{read\_entries}} + \text{Fee}_{\text{write\_entries}} + \text{Fee}_{\text{read\_bytes}}$$

*(Write byte costs are priced through the state rent mechanism based on average Soroban state size target ratios `rent_fee1_kb_soroban_state_size_low` / `high` and included in the refundable rent fee).*

### 3.4 Transaction Envelope Size (Bandwidth) Fee

From `ConfigSettingContractBandwidthV0.fee_tx_size1_kb` ($R_{\text{tx\_kb}}$), applied to the raw XDR byte length ($B_{\text{tx}}$) of the `TransactionEnvelope` (decoded from base64, never the base64 character length):

$$\text{Fee}_{\text{bandwidth}} = \left\lfloor \frac{B_{\text{tx}} \times R_{\text{tx\_kb}}}{1{,}024} \right\rfloor$$

### 3.5 Non-Refundable Resource Fee Total

$$\text{Fee}_{\text{non-ref}} = \text{Fee}_{\text{cpu}} + \text{Fee}_{\text{storage}} + \text{Fee}_{\text{bandwidth}}$$

### 3.6 Refundable Storage Rent & Events Fee

The authoritative total resource fee ($\text{Fee}_{\text{resource\_total}}$) is returned directly by Stellar RPC in `simulateTransaction` (`minResourceFee`, which equals `SorobanTransactionData.resource_fee`). It encapsulates:

- Non-refundable execution and footprint costs ($\text{Fee}_{\text{non-ref}}$)
- Contract return value and diagnostic/contract events (`ConfigSettingContractEventsV0.fee_contract_events1_kb`):
  $$\text{Fee}_{\text{events}} = \left\lceil \frac{B_{\text{events}} \times R_{\text{events\_kb}}}{1{,}024} \right\rceil$$
- Ledger state rent (`StateArchivalSettings` & `ConfigSettingContractLedgerCostV0`):
  $$\text{Fee}_{\text{rent}} = \left\lceil \frac{B_{\text{rent\_bytes}} \times \Delta\text{Ledgers} \times R_{\text{rent\_1kb}}}{1{,}024 \times D_{\text{rent\_rate}}} \right\rceil$$

Because `minResourceFee` is computed by Stellar Core's own ledger simulation engine against the live bucket list state size, `soroban-cost-estimator` derives the refundable portion as the exact non-negative remainder:

$$\text{Fee}_{\text{ref}} = \max\!\left(0,\; \text{Fee}_{\text{resource\_total}} - \text{Fee}_{\text{non-ref}}\right)$$

### 3.7 Base Transaction Inclusion Fee

Every Stellar transaction pays a minimum base inclusion fee of $100\text{ stroops}$ per operation ($B_{\text{base}} = 100$ for a single `InvokeHostFunctionOp`):

$$\text{Fee}_{\text{total}} = B_{\text{base}} + \text{Fee}_{\text{resource\_total}}$$

---

## 4. Why Estimates Match Stellar Core `simulateTransaction` Exactly

1. **Authoritative Footprint & Resource Extraction**: `soroban-cost-estimator` decodes the base64 `TransactionData` (`SorobanTransactionData`) and `RestorePreamble` returned by `simulateTransaction` using `stellar-xdr`, reading exact instruction counts, disk read/write bytes, and ledger footprint keys.
2. **Live Ledger Config Rates**: Fee component breakdowns use the exact `ConfigSettingEntry` values active on the target ledger (`fetch_all_config_settings`), never stale protocol defaults.
3. **Integer-Only Stroop Arithmetic**: All multiplications use `i128::from(units) * i128::from(rate) / i128::from(scale)` before clamping to `i64`, eliminating floating-point inaccuracies.
4. **XLM String Formatting Without Float Loss**: Total XLM is formatted directly from integer stroops using fixed-point decimal division ($1\text{ XLM} = 10^7\text{ stroops}$), ensuring exact agreement with Stellar Core and Horizon/RPC ledger records.
