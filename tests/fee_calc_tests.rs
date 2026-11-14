//! Edge-case tests for `xlm_to_stroops` parsing.
//!
//! Covers empty/whitespace input, malformed fractions, overflow, negative
//! zero, and very large values at and beyond the `i64` stroops boundary.
//! Happy-path conversions are also covered by unit tests in
//! `src/report/fee_calc.rs`.
//!
//! Issue #358 added the decimal-precision, sign, and overflow matrices below;
//! the sign matrix pins the fix for negative fractions, which were parsed by
//! adding the fraction to a negative whole part (`"-1.5"` came out as
//! `-0.5` XLM).

use soroban_cost_estimator::report::fee_calc::{stroops_to_xlm, xlm_to_stroops};

/// Stroop precision: 1 XLM = 10^7 stroops.
const STROOPS_PER_XLM: i64 = 10_000_000;

/// `i64::MIN` stroops as an XLM string — the signed mirror of [`MAX_XLM`].
const MIN_XLM: &str = "-922337203685.4775808";

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
    // anything beyond is truncated rather than rounded. This mirrors what
    // `stroops_to_xlm` emits at the default precision, so a value the CLI
    // prints round-trips back to the exact stroop count.
    assert_eq!(xlm_to_stroops("0.12345678").unwrap(), 1_234_567);
    assert_eq!(xlm_to_stroops("1.00000009").unwrap(), 10_000_000);
    // The sub-stroop digits of the issue's example are dropped, not rejected:
    // every stroop value is already representable, so there is nothing to
    // round and no precision to lose. Stricter validation would be a
    // behaviour change to this public helper, tracked separately.
    assert_eq!(xlm_to_stroops("1.00000001").unwrap(), STROOPS_PER_XLM);
}

// ─────────────────────────────────────────────────────────────────────────
// Issue #358 — decimal precision boundaries
// ─────────────────────────────────────────────────────────────────────────

/// Every accepted input from the issue's acceptance criteria, plus the
/// neighbouring shapes that exercise the same code paths.
#[test]
fn test_accepted_string_matrix() {
    for (input, expected) in [
        ("0", 0),
        ("0.1", 1_000_000),
        ("0.0000001", 1), // exactly 1 stroop
        ("1.0000000", STROOPS_PER_XLM),
        ("100.500", 1_005_000_000),
        ("100.5", 1_005_000_000),
        ("1", STROOPS_PER_XLM),
        ("1.5", 15_000_000),
        ("0.0000009", 9),        // full 7-digit fraction
        ("1.", STROOPS_PER_XLM), // an empty fraction is zero
        ("007", 70_000_000),     // leading zeros are ordinary digits
    ] {
        assert_eq!(
            xlm_to_stroops(input).expect("valid XLM value"),
            expected,
            "input {input:?}"
        );
    }
}

/// A leading `-` applies to the whole amount, so it scales the fraction too.
#[test]
fn test_negative_values_are_signed_across_the_fraction() {
    for (input, expected) in [
        ("-5", -50_000_000),
        ("-0.1", -1_000_000),
        ("-1.5", -15_000_000),
        ("-100.500", -1_005_000_000),
        ("-0.0000001", -1),
    ] {
        assert_eq!(
            xlm_to_stroops(input).expect("valid XLM value"),
            expected,
            "input {input:?}"
        );
    }

    // A bare sign carries no value and must be rejected rather than read as
    // zero.
    assert!(xlm_to_stroops("-").is_err(), "`-` alone is not a value");
}

/// Both signed extremes of the stroop range parse exactly: `i64::MAX` is the
/// largest value the parser must accept, and `i64::MIN` its mirror (whose
/// magnitude does not fit in an `i64` until the sign is applied).
#[test]
fn test_overflow_boundaries() {
    assert_eq!(xlm_to_stroops(MAX_XLM).unwrap(), i64::MAX);
    assert_eq!(xlm_to_stroops(MIN_XLM).unwrap(), i64::MIN);

    // One stroop past either end is rejected, as is a whole part that cannot
    // fit in `i64` at all.
    for input in [
        "922337203685.4775808",
        "-922337203685.4775809",
        "922337203686",
        "-922337203686",
        "99999999999999999999",
    ] {
        assert!(
            xlm_to_stroops(input).is_err(),
            "{input:?} is outside the stroop range and must be rejected"
        );
    }
}

/// The issue's error cases, plus more malformed shapes: whitespace anywhere,
/// repeated dots, non-digits, and non-ASCII input. Every one must come back
/// as an error — never a panic.
#[test]
fn test_rejected_string_matrix() {
    for input in [
        "",
        " ",
        " 1.0 ",
        "1.0 ",
        " 1.0",
        "-5 ",
        "1..0",
        "1.2.3",
        "abc",
        "12x.5",
        ".5",
        "1.x",
        "1.1234567x", // non-digit past the 7th place
        "1.ééééé",    // multi-byte UTF-8 fraction (used to panic)
        "--5",
        "1e3",
    ] {
        let err = xlm_to_stroops(input).expect_err(&format!("{input:?} must be rejected"));
        assert!(
            err.to_string().contains("invalid XLM value"),
            "{input:?} should report an invalid XLM value; got: {err}"
        );
    }
}

/// Every value the formatter emits must parse back to the same stroop count,
/// so no precision is lost in a report → parse round trip.
#[test]
fn test_round_trip_with_stroops_to_xlm() {
    for stroops in [
        0,
        1,
        -1,
        1_234_567,
        -1_234_567,
        STROOPS_PER_XLM,
        -STROOPS_PER_XLM,
        999_999_999_999_999,
        -999_999_999_999_999,
        i64::MAX,
        i64::MIN,
    ] {
        let xlm = stroops_to_xlm(stroops, 7);
        assert_eq!(
            xlm_to_stroops(&xlm).expect("formatted stroops must parse"),
            stroops,
            "round trip failed for {stroops} stroops (formatted as {xlm:?})"
        );
    }
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
