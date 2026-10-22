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

#[test]
fn test_fee_consistency_exact_match_has_no_warning() {
    use soroban_cost_estimator::report::fee_calc::{
        DEFAULT_PRECISION, FeeRates, check_fee_consistency, compute_fee_breakdown,
    };

    let rates = FeeRates {
        fee_per_10k_insns: 1_000,
        fee_per_read_entry: 500,
        fee_per_write_entry: 1_500,
        fee_per_read_1kb: 1_024,
        fee_per_1kb: 1_024,
    };
    // cpu = 1_000, read_entry = 500, write_entry = 1_500, read_bytes = 1_024, bw = 1_024 -> non_refundable = 5_048
    let breakdown =
        compute_fee_breakdown(5_048, 10_000, 1, 1, 1024, 1024, rates, DEFAULT_PRECISION);
    assert_eq!(breakdown.non_refundable_stroops, 5_048);
    assert_eq!(breakdown.authoritative_resource_fee_stroops(), 5_048);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_eq!(breakdown.consistency_warning(), None);
    assert_eq!(check_fee_consistency(5_048, 5_048), None);
}

#[test]
fn test_fee_consistency_rounding_boundary() {
    use soroban_cost_estimator::report::fee_calc::{
        DEFAULT_PRECISION, FeeRates, check_fee_consistency, compute_fee_breakdown,
    };

    let rates = FeeRates {
        fee_per_10k_insns: 100,
        fee_per_read_entry: 0,
        fee_per_write_entry: 0,
        fee_per_read_1kb: 0,
        fee_per_1kb: 0,
    };
    // 19_999 * 100 / 10_000 = 199 stroops (floor-divided at the rounding boundary).
    let exact_boundary = compute_fee_breakdown(199, 19_999, 0, 0, 0, 0, rates, DEFAULT_PRECISION);
    assert_eq!(exact_boundary.non_refundable_stroops, 199);
    assert_eq!(exact_boundary.consistency_warning(), None);

    // 20_000 * 100 / 10_000 = 200 stroops: 1 stroop above the 199 RPC total triggers warning.
    let one_over = compute_fee_breakdown(199, 20_000, 0, 0, 0, 0, rates, DEFAULT_PRECISION);
    assert_eq!(one_over.non_refundable_stroops, 200);
    assert_eq!(one_over.refundable_stroops, 0);
    assert_eq!(one_over.total_stroops, 299); // 199 resource + 100 base
    let warning = one_over
        .consistency_warning()
        .expect("1 stroop excess should warn");
    assert_eq!(warning.derived_non_refundable_stroops, 200);
    assert_eq!(warning.authoritative_resource_fee_stroops, 199);
    assert_eq!(warning.excess_stroops, 1);

    assert!(check_fee_consistency(200, 199).is_none());
}

#[test]
fn test_fee_consistency_derived_components_greater_than_total() {
    use soroban_cost_estimator::report::cost_report::format_fee_consistency_warning;
    use soroban_cost_estimator::report::fee_calc::{
        DEFAULT_PRECISION, FeeRates, compute_fee_breakdown,
    };

    let rates = FeeRates {
        fee_per_10k_insns: 1_024,
        fee_per_read_entry: 1_500,
        fee_per_write_entry: 2_500,
        fee_per_read_1kb: 512,
        fee_per_1kb: 400,
    };
    // Derived non-refundable:
    // cpu = 10_240, read = 1_500, write = 2_500, read_bytes = 512, bw = 400 -> 15_152 stroops
    // Authoritative RPC total_resource_fee = 5_000 stroops
    let breakdown =
        compute_fee_breakdown(5_000, 100_000, 1, 1, 1024, 1024, rates, DEFAULT_PRECISION);
    // Authoritative total and component arithmetic remain unchanged.
    assert_eq!(breakdown.total_stroops, 5_100);
    assert_eq!(breakdown.non_refundable_stroops, 15_152);
    assert_eq!(breakdown.refundable_stroops, 0);

    let warning = breakdown
        .consistency_warning()
        .expect("mismatch should produce structured warning");
    assert_eq!(warning.code, "derived_non_refundable_exceeds_resource_fee");
    assert_eq!(warning.derived_non_refundable_stroops, 15_152);
    assert_eq!(warning.authoritative_resource_fee_stroops, 5_000);
    assert_eq!(warning.excess_stroops, 10_152);
    assert!(warning.message.contains("15152 stroops"));
    assert!(warning.message.contains("5000 stroops"));
    assert!(warning.message.contains("10152 stroops"));
    assert!(warning.message.contains("stale fee rates"));

    let formatted = format_fee_consistency_warning(Some(&warning));
    assert!(formatted.contains("Fee consistency warning:"));
    assert!(formatted.contains("15152 stroops"));
    assert_eq!(format_fee_consistency_warning(None), "");
}
