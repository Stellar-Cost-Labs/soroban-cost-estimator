//! Edge-case tests for `xlm_to_stroops` parsing and for the fee-component
//! labels (issue #526).
//!
//! Covers empty/whitespace input, malformed fractions, overflow, negative
//! zero, and very large values at and beyond the `i64` stroops boundary.
//! Happy-path conversions are covered by unit tests in
//! `src/report/fee_calc.rs`.

use soroban_cost_estimator::report::fee_calc::{
    DEFAULT_PRECISION, FeeRates, compute_fee_breakdown, xlm_to_stroops,
};

/// Issue #526 — a response with refundable fees: every component total is
/// pinned, and the refundable remainder is exposed under a `refundable`
/// label, never as a protocol-measured `rent` fee.
///
/// Numbers mirror the live testnet cross-check (increment contract):
/// CPU = (532_502 × 7) / 10_000 = 372
/// read entry = 1 × 1_563, write entry = 1 × 2_500, disk read bytes = 0
/// storage = 1_563 + 2_500 = 4_063
/// tx size = (156 × 406) / 1_024 = 61
/// non-refundable = 372 + 4_063 + 61 = 4_496
/// refundable (derived remainder) = 15_427 − 4_496 = 10_931
#[test]
fn test_refundable_remainder_component_totals_and_labels() {
    let breakdown = compute_fee_breakdown(
        15_427,  // total_resource_fee: the authoritative RPC fee
        532_502, // cpu_insns
        1,       // read_entries
        1,       // write_entries
        0,       // read_bytes
        156,     // tx_size
        FeeRates {
            fee_per_10k_insns: 7,
            fee_per_read_entry: 1_563,
            fee_per_write_entry: 2_500,
            fee_per_read_1kb: 447,
            fee_per_1kb: 406,
        },
        DEFAULT_PRECISION,
    );

    // Component totals: integer stroops math, unchanged by the label fix.
    assert_eq!(breakdown.cpu_fee_stroops, 372);
    assert_eq!(breakdown.storage_fee_stroops, 4_063);
    assert_eq!(breakdown.bandwidth_fee_stroops, 61);
    assert_eq!(breakdown.non_refundable_stroops, 4_496);
    // The refundable line is the derived remainder — an estimate of the
    // protocol's refundable allowance, not a measured rent fee.
    assert_eq!(breakdown.refundable_stroops, 10_931);
    assert_eq!(breakdown.base_fee_stroops, 100);
    assert_eq!(breakdown.total_stroops, 15_527); // 15_427 authoritative + 100 base
    assert_eq!(breakdown.total_xlm, "0.0015527");

    // Labels: the percentage map exposes a `refundable` key for the derived
    // remainder — and never a `rent` key (issue #526).
    let keys: Vec<&str> = breakdown
        .fee_percentages
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "base_fee",
            "cpu_instructions",
            "refundable",
            "storage_read_write",
            "transaction_size"
        ],
        "BTreeMap keys are sorted; refundable replaces the old rent key"
    );
    // Shares of the 15_527-stroop total, exact to 0.1%.
    assert_eq!(breakdown.fee_percentages["cpu_instructions"], "2.4%");
    assert_eq!(breakdown.fee_percentages["storage_read_write"], "26.2%");
    assert_eq!(breakdown.fee_percentages["transaction_size"], "0.4%");
    assert_eq!(breakdown.fee_percentages["base_fee"], "0.6%");
    assert_eq!(breakdown.fee_percentages["refundable"], "70.4%");
}

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
