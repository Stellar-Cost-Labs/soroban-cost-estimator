//! Edge-case tests for `xlm_to_stroops` parsing.
//!
//! Covers empty/whitespace input, malformed fractions, overflow, negative
//! zero, and very large values at and beyond the `i64` stroops boundary.
//! Happy-path conversions are covered by unit tests in
//! `src/report/fee_calc.rs`.

use soroban_cost_estimator::report::fee_calc::xlm_to_stroops;

/// `i64::MAX` stroops expressed as an XLM string: 922337203685 XLM +
/// 4775807 stroops of fraction, exactly representable with 7 digits.
const MAX_XLM: &str = "922337203685.4775807";

#[test]
fn test_empty_string_is_rejected() {
    let err = xlm_to_stroops("").unwrap_err();
    assert!(
        err.to_string().contains("invalid XLM value"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_whitespace_only_is_rejected() {
    assert!(xlm_to_stroops("   ").is_err());
    assert!(xlm_to_stroops("\t").is_err());
}

#[test]
fn test_fraction_without_whole_part_is_rejected() {
    // The whole part is empty before the dot and cannot parse as i64.
    assert!(xlm_to_stroops(".5").is_err());
}

#[test]
fn test_multiple_dots_are_rejected() {
    assert!(xlm_to_stroops("1.2.3").is_err());
    assert!(xlm_to_stroops("1..2").is_err());
}

#[test]
fn test_non_numeric_input_is_rejected() {
    assert!(xlm_to_stroops("abc").is_err());
    assert!(xlm_to_stroops("12x.5").is_err());
    assert!(xlm_to_stroops("nan").is_err());
}

#[test]
fn test_negative_zero_parses_to_zero() {
    // "-0" parses to i64 zero; no negative-zero stroop value exists.
    assert_eq!(xlm_to_stroops("-0").unwrap(), 0);
    assert_eq!(xlm_to_stroops("-0.0").unwrap(), 0);
    assert_eq!(xlm_to_stroops("-0.0000000").unwrap(), 0);
}

#[test]
fn test_max_i64_value_is_accepted() {
    // Exactly i64::MAX stroops: the largest value that must not overflow.
    assert_eq!(xlm_to_stroops(MAX_XLM).unwrap(), i64::MAX);
}

#[test]
fn test_whole_only_overflow_is_rejected() {
    // 922337203686 * 10^7 exceeds i64::MAX → checked_mul overflow.
    let err = xlm_to_stroops("922337203686").unwrap_err();
    assert!(
        err.to_string()
            .contains("XLM value is out of range for stroops"),
        "unexpected error: {err}"
    );
    // Far beyond i64 range: the parse itself fails (reported as invalid).
    assert!(xlm_to_stroops("99999999999999999999").is_err());
}

#[test]
fn test_fractional_overflow_is_rejected() {
    // One stroop past i64::MAX: checked_add overflows on the fraction.
    let err = xlm_to_stroops("922337203685.4775808").unwrap_err();
    assert!(
        err.to_string()
            .contains("XLM value is out of range for stroops"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_very_large_values_round_trip_within_precision() {
    // Large but safely-representable values keep all 7 fractional digits.
    assert_eq!(
        xlm_to_stroops("99999999.9999999").unwrap(),
        999_999_999_999_999
    );
    assert_eq!(
        xlm_to_stroops("123456789.9876543").unwrap(),
        1_234_567_899_876_543
    );
}

#[test]
fn test_extra_fraction_digits_are_truncated() {
    // Only the first 7 fractional digits (stroop precision) are used;
    // anything beyond is silently truncated rather than rounded.
    assert_eq!(xlm_to_stroops("0.12345678").unwrap(), 1_234_567);
    assert_eq!(xlm_to_stroops("1.00000009").unwrap(), 10_000_000);
}

// ---------------------------------------------------------------------------
// Regression tests: negative refundable fee edge cases with storage I/O (#367)
// ---------------------------------------------------------------------------

use soroban_cost_estimator::report::fee_calc::{
    DEFAULT_PRECISION, FeeRates, compute_fee_breakdown,
};

fn mainnet_like_rates() -> FeeRates {
    FeeRates {
        fee_per_10k_insns: 25,
        fee_per_read_entry: 6_250,
        fee_per_write_entry: 10_000,
        fee_per_read_1kb: 1_786,
        fee_per_1kb: 16_235,
    }
}

#[test]
fn test_regression_refundable_fee_floors_at_zero_when_storage_io_exceeds_total() {
    // Historical bug: when non-refundable storage I/O fees (read_entries +
    // write_entries + read_bytes) exceeded the reported total_resource_fee,
    // subtracting non_refundable from total_resource_fee yielded a negative
    // refundable_stroops value.
    let rates = mainnet_like_rates();

    // Storage I/O alone costs: 4 * 6,250 + 2 * 10,000 + (8192 * 1786)/1024 = 25,000 + 20,000 + 14,288 = 59,288 stroops
    // Pass a smaller total_resource_fee (10,000 stroops) to trigger the underflow path.
    let breakdown = compute_fee_breakdown(
        10_000,    // total_resource_fee smaller than non-refundable storage I/O
        1_000_000, // cpu_insns (2,500 stroops)
        4,         // read_entries
        2,         // write_entries
        8_192,     // read_bytes
        512,       // tx_size
        rates,
        DEFAULT_PRECISION,
    );

    assert_eq!(
        breakdown.refundable_stroops, 0,
        "refundable_stroops must never be negative when storage I/O exceeds total_resource_fee"
    );
    assert!(breakdown.refundable_stroops >= 0);
    assert_eq!(
        breakdown.non_refundable_stroops,
        breakdown.cpu_fee_stroops + breakdown.storage_fee_stroops + breakdown.bandwidth_fee_stroops,
        "non-refundable fee must equal sum of CPU, storage I/O, and bandwidth components"
    );
    assert_eq!(
        breakdown.total_stroops,
        10_000 + breakdown.base_fee_stroops,
        "total_stroops must equal total_resource_fee + base_fee_stroops"
    );
}

#[test]
fn test_regression_refundable_fee_zero_total_with_nonzero_storage_io() {
    // Edge case where simulation omits minResourceFee (total_resource_fee = 0)
    // while ledger footprint still reports read/write entries and bytes.
    let rates = mainnet_like_rates();
    let breakdown = compute_fee_breakdown(0, 500_000, 3, 2, 4_096, 300, rates, DEFAULT_PRECISION);

    assert_eq!(breakdown.refundable_stroops, 0);
    assert_eq!(breakdown.base_fee_stroops, 0);
    assert_eq!(breakdown.total_stroops, 0);
    assert!(breakdown.storage_fee_stroops > 0);
    assert_eq!(
        breakdown.non_refundable_stroops,
        breakdown.cpu_fee_stroops + breakdown.storage_fee_stroops + breakdown.bandwidth_fee_stroops
    );
}

#[test]
fn test_regression_extreme_storage_io_high_write_rates_never_underflow() {
    // Extreme storage I/O configuration: large entry counts, maximum u32 read
    // bytes, and high per-entry / per-KB fee rates.
    let extreme_rates = FeeRates {
        fee_per_10k_insns: 100_000,
        fee_per_read_entry: 5_000_000,
        fee_per_write_entry: 25_000_000,
        fee_per_read_1kb: 10_000_000,
        fee_per_1kb: 5_000_000,
    };

    let breakdown = compute_fee_breakdown(
        50_000,           // small reported total_resource_fee
        100_000_000,      // 100M CPU instructions
        100,              // 100 read entries
        50,               // 50 write entries
        10 * 1024 * 1024, // 10 MiB read bytes
        65_536,           // 64 KiB tx size
        extreme_rates,
        DEFAULT_PRECISION,
    );

    assert!(breakdown.refundable_stroops >= 0);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_eq!(
        breakdown.non_refundable_stroops,
        breakdown.cpu_fee_stroops + breakdown.storage_fee_stroops + breakdown.bandwidth_fee_stroops
    );

    // When authoritative total_resource_fee exceeds non_refundable_stroops,
    // total_stroops matches the exact sum of all parts.
    let sufficient_total = breakdown.non_refundable_stroops + 75_000;
    let balanced = compute_fee_breakdown(
        sufficient_total,
        100_000_000,
        100,
        50,
        10 * 1024 * 1024,
        65_536,
        extreme_rates,
        DEFAULT_PRECISION,
    );
    assert_eq!(balanced.refundable_stroops, 75_000);
    assert_eq!(
        balanced.total_stroops,
        balanced.cpu_fee_stroops
            + balanced.storage_fee_stroops
            + balanced.bandwidth_fee_stroops
            + balanced.refundable_stroops
            + balanced.base_fee_stroops
    );
}
