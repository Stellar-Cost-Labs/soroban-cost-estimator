use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// The fee breakdown for a single simulation.
///
/// All values are in stroops (1 stroop = 10^{-7} XLM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeBreakdown {
    /// Non-refundable resource fee (charged regardless of success).
    pub non_refundable_stroops: i64,
    /// Refundable resource fee (refunded if not fully consumed).
    pub refundable_stroops: i64,
    /// Total resource fee.
    pub total_stroops: i64,
    /// Total fee in XLM (as a string to avoid float precision issues).
    pub total_xlm: String,
}

/// Fee rates sourced from the network's `ConfigSettingContract*` entries.
///
/// All rates are raw config values: stroops per 10,000 instructions
/// (`fee_rate_per_instructions_increment`), per ledger entry
/// (`fee_disk_read_ledger_entry` / `fee_write_ledger_entry`), and per 1KB
/// (`fee_disk_read1_kb` / `fee_tx_size1_kb`).
#[derive(Debug, Clone, Copy)]
pub struct FeeRates {
    /// Stroops per 10,000 CPU instructions.
    pub fee_per_10k_insns: i64,
    /// Stroops per ledger entry read.
    pub fee_per_read_entry: i64,
    /// Stroops per ledger entry written.
    pub fee_per_write_entry: i64,
    /// Stroops per 1KB of disk read bytes.
    pub fee_per_read_1kb: i64,
    /// Stroops per 1KB of transaction size.
    pub fee_per_1kb: i64,
}

/// Compute the fee breakdown from a simulation result.
///
/// The total resource fee comes from the `simulateTransaction` response
/// (the `minResourceFee` field, or the `resource_fee` inside the returned
/// `SorobanTransactionData`). Per the XDR definition of
/// `SorobanTransactionData.resource_fee`, that total is made of:
///
/// - a **non-refundable** portion: fees for CPU instructions, ledger I/O
///   and transaction size — derived independently here from the network's
///   own config-sourced rates;
/// - a **refundable** portion: the remainder of the authoritative total,
///   covering events and ledger rent bumps.
///
/// The fee rates from the network config are stored per 10K instructions
/// and per 1KB. The formula `(units * rate) / scale` preserves precision
/// better than pre-dividing the rate.
///
/// # Arguments
/// * `total_resource_fee` - The authoritative total Soroban resource fee
///   from `simulateTransaction`, in stroops. For a successful simulation
///   this is `minResourceFee` (which equals `transaction_data.resource_fee`).
/// * `cpu_insns` - CPU instructions consumed.
/// * `read_entries` / `write_entries` - Ledger entries read / written.
/// * `read_bytes` - Disk bytes read.
/// * `tx_size` - Transaction size in **XDR bytes** (not base64 characters).
/// * `rates` - Config-sourced fee rates (see `FeeRates`).
///
/// # Network calls
/// None — pure computation.
#[must_use]
pub fn compute_fee_breakdown(
    total_resource_fee: i64,
    cpu_insns: u64,
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    tx_size: u32,
    rates: FeeRates,
) -> FeeBreakdown {
    // CPU fee: stroops per 10K instructions → (cpu_insns * rate) / 10000
    let cpu_fee = ((cpu_insns as i64)
        .checked_mul(rates.fee_per_10k_insns)
        .unwrap_or(i64::MAX))
        / 10_000;

    // Storage I/O fees (non-refundable): per-entry read/write fees and a
    // per-KB fee on disk bytes read.
    let read_entry_fee = (read_entries as i64).saturating_mul(rates.fee_per_read_entry);
    let write_entry_fee = (write_entries as i64).saturating_mul(rates.fee_per_write_entry);
    let read_bytes_fee = ((read_bytes as i64)
        .checked_mul(rates.fee_per_read_1kb)
        .unwrap_or(i64::MAX))
        / 1024;

    // Bandwidth fee: stroops per 1KB → (tx_size * rate) / 1024
    let bandwidth_fee = ((tx_size as i64)
        .checked_mul(rates.fee_per_1kb)
        .unwrap_or(i64::MAX))
        / 1024;

    // Non-refundable: CPU + storage I/O + bandwidth fees, computed
    // independently from the config-sourced rates. This is NOT clamped to
    // the total: capping it here would silently hide errors in the rest of
    // the chain (wrong tx size, wrong rate lookup) by reporting a smaller
    // fee than the rates imply.
    let non_refundable = cpu_fee
        .saturating_add(read_entry_fee)
        .saturating_add(write_entry_fee)
        .saturating_add(read_bytes_fee)
        .saturating_add(bandwidth_fee);

    // Refundable: the remainder of the authoritative total after the
    // non-refundable costs (events, ledger rent bumps). `.max(0)` floors
    // the one legitimate edge case: a simulation response that omits the
    // fee entirely (callers pass 0 for the total) while the config rates
    // still yield a positive non-refundable fee. In that case the
    // refundable must not go negative — it would report an impossible
    // value in the cost report. For any real simulation the RPC returns a
    // total >= the non-refundable portion, so this is a defensive floor,
    // not a mask. (`saturating_sub` alone is not enough: it saturates at
    // `i64::MIN`, not at zero.)
    let refundable = total_resource_fee.saturating_sub(non_refundable).max(0);

    let total_xlm = stroops_to_xlm(total_resource_fee);

    FeeBreakdown {
        non_refundable_stroops: non_refundable,
        refundable_stroops: refundable,
        total_stroops: total_resource_fee,
        total_xlm,
    }
}

/// Convert stroops to an XLM string (1 XLM = 10^7 stroops).
///
/// Returns a string to avoid floating-point precision issues.
/// Example: 1234567 stroops → "0.1234567"
#[must_use]
pub fn stroops_to_xlm(stroops: i64) -> String {
    let abs = stroops.unsigned_abs();
    let whole = abs / 10_000_000;
    let fraction = abs % 10_000_000;

    if stroops < 0 {
        format!("-{whole}.{fraction:07}")
    } else {
        format!("{whole}.{fraction:07}")
    }
}

/// Parse an XLM string to stroops (i64).
pub fn xlm_to_stroops(xlm: &str) -> AppResult<i64> {
    let parts: Vec<&str> = xlm.split('.').collect();
    match parts.len() {
        1 => {
            let whole: i64 = parts[0]
                .parse()
                .map_err(|_| AppError::FeeCalc(format!("invalid XLM value: {xlm}")))?;
            whole
                .checked_mul(10_000_000)
                .ok_or_else(|| AppError::FeeCalc("XLM value overflow".to_string()))
        }
        2 => {
            let whole: i64 = parts[0]
                .parse()
                .map_err(|_| AppError::FeeCalc(format!("invalid XLM value: {xlm}")))?;
            let fraction_str = format!("{:0<7}", parts[1]);
            let fraction: i64 = fraction_str[..7.min(fraction_str.len())]
                .parse()
                .map_err(|_| AppError::FeeCalc(format!("invalid XLM value: {xlm}")))?;
            whole
                .checked_mul(10_000_000)
                .and_then(|w| w.checked_add(fraction))
                .ok_or_else(|| AppError::FeeCalc("XLM value overflow".to_string()))
        }
        _ => Err(AppError::FeeCalc(format!("invalid XLM value: {xlm}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rates with zero storage I/O fees — isolates the CPU + bandwidth
    /// portion (the original pre-storage-fee behavior).
    fn cpu_and_bandwidth_only_rates() -> FeeRates {
        FeeRates {
            fee_per_10k_insns: 1024,
            fee_per_read_entry: 0,
            fee_per_write_entry: 0,
            fee_per_read_1kb: 0,
            fee_per_1kb: 10,
        }
    }

    #[test]
    fn test_zero_resource_fee_does_not_produce_negative_refundable() {
        // Regression: this input used to produce a negative refundable
        // (`0 - 386 = -386`) because non_refundable was clamped to the total
        // and then subtracted. Now the non-refundable is derived from the
        // config rates independently, and the refundable floors at 0.
        let breakdown = compute_fee_breakdown(
            0,       // total_resource_fee = 0 (fee omitted by the RPC)
            100_000, // cpu_insns
            0,       // read_entries
            0,       // write_entries
            0,       // read_bytes
            1024,    // tx_size
            cpu_and_bandwidth_only_rates(),
        );
        assert_eq!(breakdown.total_stroops, 0);
        assert_eq!(breakdown.total_xlm, "0.0000000");
        // CPU 10_240 + bandwidth 10, uncapped: the config rates say this is
        // the true non-refundable cost.
        assert_eq!(breakdown.non_refundable_stroops, 10_250);
        assert_eq!(
            breakdown.refundable_stroops, 0,
            "refundable should never be negative"
        );
    }

    #[test]
    fn test_non_refundable_not_clamped_to_total() {
        // The clamp removed: when the authoritative total is smaller than the
        // config-derived non-refundable fee, the non-refundable is reported at
        // its true rate-derived value instead of being capped, and the
        // refundable floors at 0. Capping here would silently hide unit or
        // rate-lookup bugs elsewhere in the chain.
        let breakdown = compute_fee_breakdown(
            5_000,   // total_resource_fee < cpu+bw
            100_000, // cpu_insns
            0,       // read_entries
            0,       // write_entries
            0,       // read_bytes
            1024,    // tx_size
            cpu_and_bandwidth_only_rates(),
        );
        assert_eq!(breakdown.total_stroops, 5_000);
        assert_eq!(breakdown.non_refundable_stroops, 10_250);
        assert_eq!(breakdown.refundable_stroops, 0);
    }

    #[test]
    fn test_stroops_to_xlm() {
        assert_eq!(stroops_to_xlm(0), "0.0000000");
        assert_eq!(stroops_to_xlm(10_000_000), "1.0000000");
        assert_eq!(stroops_to_xlm(1_234_567), "0.1234567");
        assert_eq!(stroops_to_xlm(-10_000_000), "-1.0000000");
    }

    #[test]
    fn test_xlm_to_stroops() {
        assert_eq!(xlm_to_stroops("0.0000000").unwrap(), 0);
        assert_eq!(xlm_to_stroops("1.0000000").unwrap(), 10_000_000);
        assert_eq!(xlm_to_stroops("0.1234567").unwrap(), 1_234_567);
        assert!(xlm_to_stroops("invalid").is_err());
    }

    #[test]
    fn test_compute_fee_breakdown() {
        // CPU fee = (100_000 * 1024) / 10_000 = 10_240
        // Bandwidth fee = (1024 * 10) / 1024 = 10
        // Non-refundable = 10_240 + 10 = 10_250
        // Refundable = 1_000_000 - 10_250 = 989_750
        let breakdown = compute_fee_breakdown(
            1_000_000, // total_resource_fee
            100_000,   // cpu_insns
            0,         // read_entries
            0,         // write_entries
            0,         // read_bytes
            1024,      // tx_size
            cpu_and_bandwidth_only_rates(),
        );
        assert_eq!(breakdown.total_stroops, 1_000_000);
        assert_eq!(breakdown.total_xlm, "0.1000000");
        assert_eq!(breakdown.non_refundable_stroops, 10_250);
        assert_eq!(breakdown.refundable_stroops, 989_750);
    }

    /// Storage I/O fees are part of the non-refundable portion, matching the
    /// live testnet cross-check (increment contract, step=5):
    /// CPU fee = (532_502 * 7) / 10_000 = 372
    /// read entry fee = 1 * 1_563 = 1_563
    /// write entry fee = 1 * 2_500 = 2_500
    /// tx size fee = (156 * 406) / 1024 = 61
    /// Non-refundable = 372 + 1_563 + 2_500 + 61 = 4_496
    /// Refundable = 15_427 - 4_496 = 10_931
    #[test]
    fn test_storage_io_fees_included_in_non_refundable() {
        let breakdown = compute_fee_breakdown(
            15_427,  // total_resource_fee from the live simulation
            532_502, // cpu_insns
            1,       // read_entries (contract code)
            1,       // write_entries (counter)
            0,       // read_bytes
            156,     // tx_size
            FeeRates {
                fee_per_10k_insns: 7,
                fee_per_read_entry: 1_563,
                fee_per_write_entry: 2_500,
                fee_per_read_1kb: 447,
                fee_per_1kb: 406,
            },
        );
        assert_eq!(breakdown.non_refundable_stroops, 4_496);
        assert_eq!(breakdown.refundable_stroops, 15_427 - 4_496);
        assert_eq!(breakdown.total_stroops, 15_427);
    }
}
