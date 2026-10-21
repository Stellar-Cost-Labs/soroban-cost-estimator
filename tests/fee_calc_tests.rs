//! Edge-case tests for `xlm_to_stroops` parsing and for the fee consistency
//! check (#528).
//!
//! Covers empty/whitespace input, malformed fractions, overflow, negative
//! zero, and very large values at and beyond the `i64` stroops boundary.
//! Happy-path conversions are covered by unit tests in
//! `src/report/fee_calc.rs`.
//!
//! The consistency tests pin the three boundary cases from issue #528: an
//! exact match between the derived non-refundable components and the
//! authoritative simulation resource fee, a rounding boundary where floor
//! division lands exactly on the total, and a genuine mismatch where the
//! derived components exceed the total.

use soroban_cost_estimator::report::fee_calc::{
    DEFAULT_PRECISION, FeeRates, check_fee_consistency, compute_fee_breakdown, xlm_to_stroops,
};

/// Rates with zero storage I/O fees — isolates the CPU + bandwidth portion
/// so the derived non-refundable total is computable by hand.
fn cpu_and_bandwidth_only_rates() -> FeeRates {
    FeeRates {
        fee_per_10k_insns: 1024,
        fee_per_read_entry: 0,
        fee_per_write_entry: 0,
        fee_per_read_1kb: 0,
        fee_per_1kb: 10,
    }
}

/// Issue #528 — exact match: derived components equal to the authoritative
/// simulation resource fee are consistent, so no warning is produced (the
/// comparison is strictly `>`).
#[test]
fn test_exact_match_produces_no_consistency_warning() {
    // CPU = (100_000 * 1024) / 10_000 = 10_240
    // Bandwidth = (1024 * 10) / 1024 = 10
    // Derived non-refundable = 10_250 == the authoritative total below.
    let breakdown = compute_fee_breakdown(
        10_250,  // total_resource_fee
        100_000, // cpu_insns
        0,       // read_entries
        0,       // write_entries
        0,       // read_bytes
        1024,    // tx_size
        cpu_and_bandwidth_only_rates(),
        DEFAULT_PRECISION,
    );
    assert_eq!(breakdown.non_refundable_stroops, 10_250);
    // The authoritative RPC total is recoverable from the breakdown.
    assert_eq!(breakdown.total_stroops - breakdown.base_fee_stroops, 10_250);
    assert!(
        check_fee_consistency(&breakdown).is_none(),
        "an exact match must not warn"
    );
}

/// Issue #528 — rounding boundary: `scaled_fee` floors, so a derived
/// component can land exactly on the authoritative total even though the
/// unrounded rate math sits a fraction above it. That boundary stays
/// warning-free; one stroop past it is reported.
#[test]
fn test_rounding_boundary_at_total_stays_consistent() {
    // (10_001 * 1024) / 10_000 = 1024.1024 → floors to 1024, exactly the
    // authoritative total; no bandwidth fee (tx_size = 0).
    let at_boundary = compute_fee_breakdown(
        1_024,  // total_resource_fee
        10_001, // cpu_insns
        0,      // read_entries
        0,      // write_entries
        0,      // read_bytes
        0,      // tx_size
        cpu_and_bandwidth_only_rates(),
        DEFAULT_PRECISION,
    );
    assert_eq!(at_boundary.non_refundable_stroops, 1_024);
    assert!(
        check_fee_consistency(&at_boundary).is_none(),
        "floor division landing on the total is an exact match"
    );

    // (10_010 * 1024) / 10_000 = 1025.024 → 1025: one stroop past the
    // boundary, which must be surfaced rather than clamped.
    let past_boundary = compute_fee_breakdown(
        1_024,  // total_resource_fee
        10_010, // cpu_insns
        0,      // read_entries
        0,      // write_entries
        0,      // read_bytes
        0,      // tx_size
        cpu_and_bandwidth_only_rates(),
        DEFAULT_PRECISION,
    );
    assert_eq!(past_boundary.non_refundable_stroops, 1_025);
    let warning = check_fee_consistency(&past_boundary)
        .expect("one stroop above the authoritative total must warn");
    assert_eq!(warning.excess_stroops, 1);
}

/// Issue #528 — derived components greater than the total: detection reports
/// the relevant values with a neutral explanation while the authoritative
/// total and the component arithmetic stay exactly as computed.
#[test]
fn test_derived_components_exceeding_total_produces_warning() {
    let breakdown = compute_fee_breakdown(
        5_000,   // total_resource_fee < derived components
        100_000, // cpu_insns → 10_240
        0,       // read_entries
        0,       // write_entries
        0,       // read_bytes
        1024,    // tx_size → 10
        cpu_and_bandwidth_only_rates(),
        DEFAULT_PRECISION,
    );

    // Detection changes nothing: the derived components stay at their true
    // rate-derived value and the authoritative total keeps the fixed base fee
    // on top, unchanged.
    assert_eq!(breakdown.non_refundable_stroops, 10_250);
    assert_eq!(breakdown.total_stroops, 5_100); // 5_000 authoritative + 100 base
    assert_eq!(breakdown.refundable_stroops, 0);

    let warning = check_fee_consistency(&breakdown)
        .expect("derived components above the authoritative total must warn");
    assert_eq!(warning.rpc_total_stroops, 5_000);
    assert_eq!(warning.derived_non_refundable_stroops, 10_250);
    assert_eq!(warning.excess_stroops, 5_250);

    // The explanation carries the values and stays neutral: it does not
    // assert the mismatch is a network bug.
    let message = &warning.message;
    assert!(
        message.contains("10250") && message.contains("5000") && message.contains("5250"),
        "message must carry the values: {message}"
    );
    assert!(
        message.contains("keeps the authoritative total unchanged"),
        "message must state the total is untouched: {message}"
    );
    assert!(
        !message.to_lowercase().contains("bug"),
        "message must stay neutral: {message}"
    );
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
