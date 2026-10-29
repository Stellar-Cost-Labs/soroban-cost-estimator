//! Edge-case tests for the fee calculator.
//!
//! * `xlm_to_stroops` parsing: empty/whitespace input, malformed fractions,
//!   overflow, negative zero, and very large values at and beyond the `i64`
//!   stroops boundary.
//! * `compute_fee_breakdown` refundable-fee regressions: the historical
//!   input that once reported a negative refundable fee, and storage I/O
//!   fees large enough to exceed the authoritative total.
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

// ---------------------------------------------------------------------------
// Refundable fee regressions (issue #367)
// ---------------------------------------------------------------------------

/// Testnet fee rates from the live increment-contract cross-check in
/// `src/report/fee_calc.rs`.
fn testnet_rates() -> FeeRates {
    FeeRates {
        fee_per_10k_insns: 7,
        fee_per_read_entry: 1_563,
        fee_per_write_entry: 2_500,
        fee_per_read_1kb: 447,
        fee_per_1kb: 406,
    }
}

/// Invariants every breakdown must hold, whatever the inputs:
///
/// * the refundable fee is never negative;
/// * the non-refundable fee is exactly CPU + storage I/O + bandwidth;
/// * the refundable fee is the remainder of the total, floored at zero;
/// * when the total covers the non-refundable fee, the two portions add
///   back up to the total exactly;
/// * the reported total is the resource fee plus the base inclusion fee.
///
/// The remainder is checked in `i128` so the expected value cannot itself
/// overflow on extreme inputs.
fn assert_refundable_invariants(total_resource_fee: i64, breakdown: &FeeBreakdown) {
    assert!(
        breakdown.refundable_stroops >= 0,
        "refundable fee went negative: {}",
        breakdown.refundable_stroops
    );

    assert_eq!(
        breakdown.non_refundable_stroops,
        breakdown
            .cpu_fee_stroops
            .saturating_add(breakdown.storage_fee_stroops)
            .saturating_add(breakdown.bandwidth_fee_stroops),
        "non-refundable must equal cpu + storage + bandwidth"
    );

    let remainder = i128::from(total_resource_fee) - i128::from(breakdown.non_refundable_stroops);
    assert_eq!(i128::from(breakdown.refundable_stroops), remainder.max(0));

    if remainder >= 0 {
        assert_eq!(
            i128::from(breakdown.non_refundable_stroops) + i128::from(breakdown.refundable_stroops),
            i128::from(total_resource_fee),
            "non-refundable + refundable must sum to the total resource fee"
        );
    }

    assert_eq!(
        breakdown.total_stroops,
        total_resource_fee.saturating_add(breakdown.base_fee_stroops)
    );

    // The rent share is derived from the refundable fee, so a negative
    // refundable would surface here as a negative percentage.
    let rent_share = &breakdown.fee_percentages["rent"];
    assert!(
        !rent_share.starts_with('-'),
        "rent share went negative: {rent_share}"
    );
}

/// The exact input behind the original bug (CID-021 in the conversation log).
///
/// A 976-byte WASM upload simulation came back with no `minResourceFee`, so
/// the caller passed a total of 0. The testnet bandwidth rate still priced
/// the transaction at `976 * 406 / 1024 = 386` stroops non-refundable, and
/// the old `total - non_refundable` produced `Refundable: -386 stroops`.
#[test]
fn test_historical_upload_input_does_not_report_negative_refundable() {
    let breakdown = compute_fee_breakdown(
        0,   // total_resource_fee (omitted by the upload simulation)
        0,   // cpu_insns
        0,   // read_entries
        0,   // write_entries
        0,   // read_bytes
        976, // tx_size
        testnet_rates(),
        DEFAULT_PRECISION,
    );

    assert_eq!(breakdown.bandwidth_fee_stroops, 386);
    assert_eq!(breakdown.non_refundable_stroops, 386);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_eq!(breakdown.total_stroops, 0);
    assert_eq!(breakdown.total_xlm, "0.0000000");
    assert_refundable_invariants(0, &breakdown);
}

/// The same zero-total upload path once the contract-code write is priced
/// in: storage I/O pushes the non-refundable fee further above the total,
/// which must still floor the refundable at zero rather than at `-2_886`.
#[test]
fn test_historical_upload_with_storage_write_floors_refundable() {
    let breakdown = compute_fee_breakdown(
        0,   // total_resource_fee
        0,   // cpu_insns
        0,   // read_entries
        1,   // write_entries (the uploaded contract code)
        0,   // read_bytes
        976, // tx_size
        testnet_rates(),
        DEFAULT_PRECISION,
    );

    assert_eq!(breakdown.storage_fee_stroops, 2_500);
    assert_eq!(breakdown.non_refundable_stroops, 2_886);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_refundable_invariants(0, &breakdown);
}

/// Storage fees alone exceed a real, non-zero total: 50 ledger writes at
/// the testnet write rate cost 125_000 stroops against a 15_427-stroop
/// simulated total. The non-refundable fee stays at its rate-derived value
/// (it is deliberately not clamped) and the refundable floors at zero.
#[test]
fn test_storage_fee_exceeding_total_floors_refundable() {
    let total_resource_fee = 15_427;
    let breakdown = compute_fee_breakdown(
        total_resource_fee,
        532_502, // cpu_insns
        1,       // read_entries
        50,      // write_entries
        0,       // read_bytes
        156,     // tx_size
        testnet_rates(),
        DEFAULT_PRECISION,
    );

    // 1 * 1_563 + 50 * 2_500
    assert_eq!(breakdown.storage_fee_stroops, 126_563);
    assert!(breakdown.storage_fee_stroops > total_resource_fee);
    // 372 cpu + 126_563 storage + 61 bandwidth
    assert_eq!(breakdown.non_refundable_stroops, 126_996);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_eq!(breakdown.total_stroops, 15_527);
    assert_refundable_invariants(total_resource_fee, &breakdown);
}

/// Off-by-one boundary around the point where the non-refundable fee
/// (4_496 stroops for the live increment cross-check) uses up the whole
/// total.
#[test]
fn test_refundable_boundary_around_non_refundable_total() {
    let cases = [
        (4_495, 0), // one stroop short: floored, not -1
        (4_496, 0), // exactly consumed
        (4_497, 1), // one stroop left over
    ];

    for (total_resource_fee, expected_refundable) in cases {
        let breakdown = compute_fee_breakdown(
            total_resource_fee,
            532_502, // cpu_insns
            1,       // read_entries
            1,       // write_entries
            0,       // read_bytes
            156,     // tx_size
            testnet_rates(),
            DEFAULT_PRECISION,
        );

        assert_eq!(breakdown.non_refundable_stroops, 4_496);
        assert_eq!(
            breakdown.refundable_stroops, expected_refundable,
            "total_resource_fee = {total_resource_fee}"
        );
        assert_refundable_invariants(total_resource_fee, &breakdown);
    }
}

/// The maximum ledger write count at the maximum write rate produces a fee
/// far beyond the `i64` range. The write fee saturates at `i64::MAX`
/// instead of wrapping negative, and the refundable floors at zero.
#[test]
fn test_max_write_entries_at_max_write_rate_saturates() {
    let total_resource_fee = 15_427;
    let rates = FeeRates {
        fee_per_write_entry: i64::MAX,
        ..testnet_rates()
    };
    let breakdown = compute_fee_breakdown(
        total_resource_fee,
        532_502,  // cpu_insns
        1,        // read_entries
        u32::MAX, // write_entries
        0,        // read_bytes
        156,      // tx_size
        rates,
        DEFAULT_PRECISION,
    );

    assert_eq!(breakdown.storage_fee_stroops, i64::MAX);
    assert_eq!(breakdown.non_refundable_stroops, i64::MAX);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_eq!(breakdown.total_stroops, 15_527);
    assert_refundable_invariants(total_resource_fee, &breakdown);
}

/// Large storage I/O that stays inside the `i64` range, so the components
/// are summed exactly rather than saturated: every possible disk read byte
/// at 10^9 stroops/KB plus every possible ledger write at 10_000 stroops
/// each, against a 100 XLM total.
#[test]
fn test_large_storage_io_without_saturation() {
    let total_resource_fee = 1_000_000_000;
    let rates = FeeRates {
        fee_per_write_entry: 10_000,
        fee_per_read_1kb: 1_000_000_000,
        ..testnet_rates()
    };
    let breakdown = compute_fee_breakdown(
        total_resource_fee,
        0,        // cpu_insns
        0,        // read_entries
        u32::MAX, // write_entries
        u32::MAX, // read_bytes
        0,        // tx_size
        rates,
        DEFAULT_PRECISION,
    );

    // u32::MAX * 10_000 = 42_949_672_950_000
    // u32::MAX * 1_000_000_000 / 1024 = 4_194_303_999_023_437
    assert_eq!(breakdown.storage_fee_stroops, 4_237_253_671_973_437);
    assert_eq!(breakdown.non_refundable_stroops, 4_237_253_671_973_437);
    assert_eq!(breakdown.refundable_stroops, 0);
    assert_refundable_invariants(total_resource_fee, &breakdown);
}

/// With the total itself at `i64::MAX`, a saturated storage fee consumes it
/// exactly (refundable 0), while an unsaturated one leaves a positive
/// remainder that still adds back up to the total.
#[test]
fn test_max_total_with_extreme_storage_io() {
    let saturated = compute_fee_breakdown(
        i64::MAX,
        0,        // cpu_insns
        0,        // read_entries
        u32::MAX, // write_entries
        0,        // read_bytes
        0,        // tx_size
        FeeRates {
            fee_per_write_entry: i64::MAX,
            ..testnet_rates()
        },
        DEFAULT_PRECISION,
    );
    assert_eq!(saturated.non_refundable_stroops, i64::MAX);
    assert_eq!(saturated.refundable_stroops, 0);
    assert_eq!(saturated.total_stroops, i64::MAX);
    assert_refundable_invariants(i64::MAX, &saturated);

    let unsaturated = compute_fee_breakdown(
        i64::MAX,
        0,        // cpu_insns
        0,        // read_entries
        u32::MAX, // write_entries
        0,        // read_bytes
        0,        // tx_size
        FeeRates {
            fee_per_write_entry: 10_000,
            ..testnet_rates()
        },
        DEFAULT_PRECISION,
    );
    assert_eq!(unsaturated.storage_fee_stroops, 42_949_672_950_000);
    assert_eq!(
        unsaturated.refundable_stroops,
        i64::MAX - 42_949_672_950_000
    );
    assert_refundable_invariants(i64::MAX, &unsaturated);
}

/// A negative total (never returned by a well-behaved RPC, but reachable
/// through a malformed response) must not leak through as a negative
/// refundable, including at `i64::MIN` where `saturating_sub` alone would
/// stick at `i64::MIN`.
#[test]
fn test_negative_total_with_storage_io_floors_refundable() {
    for total_resource_fee in [-1, -15_427, i64::MIN] {
        let breakdown = compute_fee_breakdown(
            total_resource_fee,
            532_502, // cpu_insns
            1,       // read_entries
            1,       // write_entries
            1_024,   // read_bytes
            156,     // tx_size
            testnet_rates(),
            DEFAULT_PRECISION,
        );

        assert_eq!(
            breakdown.refundable_stroops, 0,
            "total_resource_fee = {total_resource_fee}"
        );
        assert_eq!(breakdown.base_fee_stroops, 0);
        assert_refundable_invariants(total_resource_fee, &breakdown);
    }
}

/// Control case: the live testnet cross-check, where the total comfortably
/// covers storage I/O, splits into parts that sum back to the total.
#[test]
fn test_live_testnet_breakdown_sums_to_total() {
    let total_resource_fee = 15_427;
    let breakdown = compute_fee_breakdown(
        total_resource_fee,
        532_502, // cpu_insns
        1,       // read_entries
        1,       // write_entries
        0,       // read_bytes
        156,     // tx_size
        testnet_rates(),
        DEFAULT_PRECISION,
    );

    assert_eq!(breakdown.cpu_fee_stroops, 372);
    assert_eq!(breakdown.storage_fee_stroops, 4_063);
    assert_eq!(breakdown.bandwidth_fee_stroops, 61);
    assert_eq!(breakdown.refundable_stroops, 10_931);
    assert_eq!(breakdown.total_stroops, 15_527);
    assert_refundable_invariants(total_resource_fee, &breakdown);
}
