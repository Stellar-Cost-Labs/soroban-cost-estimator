//! Property-based tests for the fee calculator.
//!
//! These complement the deterministic unit tests in `src/report/fee_calc.rs`
//! by checking invariants that must hold for *any* input, not just the
//! hand-picked cases:
//!
//! * the refundable portion is never negative (the defensive floor in
//!   `compute_fee_breakdown`),
//! * the reported totals are internally consistent and round-trip through
//!   the XLM string representation,
//! * the non-refundable fee is a commutative sum of independent component
//!   fees (swapping a quantity with its rate does not change the result).

use proptest::prelude::*;

use soroban_cost_estimator::report::fee_calc::{
    DEFAULT_PRECISION, FeeRates, compute_fee_breakdown, stroops_to_xlm, xlm_to_stroops,
};

/// Strategy for fee rates in a realistic range (all non-negative).
fn realistic_rates() -> impl Strategy<Value = FeeRates> {
    (
        0..10_000i64,
        0..10_000i64,
        0..10_000i64,
        0..10_000i64,
        0..10_000i64,
    )
        .prop_map(
            |(
                fee_per_10k_insns,
                fee_per_read_entry,
                fee_per_write_entry,
                fee_per_read_1kb,
                fee_per_1kb,
            )| {
                FeeRates {
                    fee_per_10k_insns,
                    fee_per_read_entry,
                    fee_per_write_entry,
                    fee_per_read_1kb,
                    fee_per_1kb,
                }
            },
        )
}

/// Strategy for arbitrary (possibly negative) fee rates — stresses the
/// saturating arithmetic and the refundable floor with extreme values.
fn arbitrary_rates() -> impl Strategy<Value = FeeRates> {
    (
        any::<i64>(),
        any::<i64>(),
        any::<i64>(),
        any::<i64>(),
        any::<i64>(),
    )
        .prop_map(
            |(
                fee_per_10k_insns,
                fee_per_read_entry,
                fee_per_write_entry,
                fee_per_read_1kb,
                fee_per_1kb,
            )| {
                FeeRates {
                    fee_per_10k_insns,
                    fee_per_read_entry,
                    fee_per_write_entry,
                    fee_per_read_1kb,
                    fee_per_1kb,
                }
            },
        )
}

/// Strategy for realistic `(cpu_insns, read_entries, write_entries,
/// read_bytes, tx_size)` quantities. Kept small enough that the scaled fee
/// products cannot overflow `i64` in debug builds.
fn realistic_quantities() -> impl Strategy<Value = (u64, u32, u32, u32, u32)> {
    (
        0..1_000_000_000u64, // cpu_insns
        0..100_000u32,       // read_entries
        0..100_000u32,       // write_entries
        0..10_000_000u32,    // read_bytes
        0..10_000_000u32,    // tx_size
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// The refundable portion is floored at zero regardless of the inputs:
    /// even when the authoritative total is smaller than the rate-derived
    /// non-refundable fee (or the RPC omits the fee entirely), the report
    /// must never show a negative refundable. Runs with arbitrary —
    /// including negative — rates, extreme quantities, and totals.
    #[test]
    fn refundable_never_negative(
        total_resource_fee in any::<i64>(),
        quantities in any::<(u64, u32, u32, u32, u32)>(),
        rates in arbitrary_rates(),
    ) {
        let (cpu_insns, read_entries, write_entries, read_bytes, tx_size) = quantities;
        let breakdown = compute_fee_breakdown(
            total_resource_fee,
            cpu_insns,
            read_entries,
            write_entries,
            read_bytes,
            tx_size,
            rates,
            DEFAULT_PRECISION,
        );

        prop_assert!(
            breakdown.refundable_stroops >= 0,
            "refundable must never be negative, got {}",
            breakdown.refundable_stroops
        );
        // The authoritative total is always reported verbatim.
        prop_assert_eq!(breakdown.total_stroops, total_resource_fee);
        prop_assert_eq!(breakdown.total_xlm, stroops_to_xlm(total_resource_fee, DEFAULT_PRECISION));
    }

    /// The reported totals are internally consistent for any realistic
    /// input: the refundable is exactly the remainder after the
    /// non-refundable costs (floored at zero), the two portions always
    /// reconcile with the authoritative total, and the XLM string
    /// round-trips back to stroops.
    #[test]
    fn totals_consistent(
        total_resource_fee in 0..1_000_000_000i64,
        quantities in realistic_quantities(),
        rates in realistic_rates(),
    ) {
        let (cpu_insns, read_entries, write_entries, read_bytes, tx_size) = quantities;
        let breakdown = compute_fee_breakdown(
            total_resource_fee,
            cpu_insns,
            read_entries,
            write_entries,
            read_bytes,
            tx_size,
            rates,
            DEFAULT_PRECISION,
        );
        let non_refundable = breakdown.non_refundable_stroops;

        // Refundable = remainder of the total after non-refundable costs,
        // floored at zero for the fee-omitted edge case.
        prop_assert_eq!(
            breakdown.refundable_stroops,
            total_resource_fee.saturating_sub(non_refundable).max(0)
        );
        // The two portions always reconcile with the authoritative total:
        // when the total covers the non-refundable fee the sum equals the
        // total, otherwise the refundable floors at zero and the sum equals
        // the non-refundable fee.
        prop_assert_eq!(
            non_refundable + breakdown.refundable_stroops,
            total_resource_fee.max(non_refundable)
        );
        prop_assert_eq!(breakdown.total_stroops, total_resource_fee);
        prop_assert_eq!(&breakdown.total_xlm, &stroops_to_xlm(total_resource_fee, DEFAULT_PRECISION));
        // The XLM string representation round-trips back to the exact
        // stroop count.
        prop_assert_eq!(
            xlm_to_stroops(&breakdown.total_xlm).unwrap(),
            total_resource_fee
        );
    }

    /// The non-refundable fee is a commutative sum of independent component
    /// fees. Swapping a component's quantity with its rate — read/write
    /// entries and their per-entry rates, disk-read bytes and tx size with
    /// their per-1KB rates — must not change the breakdown.
    ///
    /// Runs with realistic (non-negative, non-saturating) rates: the
    /// saturating arithmetic used at extreme values is exercised by
    /// `refundable_never_negative`, but the sum of saturated intermediates
    /// is not order-independent, so commutativity is only asserted in the
    /// normal regime.
    #[test]
    fn component_fees_commute(
        total_resource_fee in any::<i64>(),
        quantities in realistic_quantities(),
        rates in realistic_rates(),
    ) {
        let (cpu_insns, read_entries, write_entries, read_bytes, tx_size) = quantities;

        let swapped_rates = FeeRates {
            fee_per_read_entry: rates.fee_per_write_entry,
            fee_per_write_entry: rates.fee_per_read_entry,
            fee_per_read_1kb: rates.fee_per_1kb,
            fee_per_1kb: rates.fee_per_read_1kb,
            ..rates
        };

        let original = compute_fee_breakdown(
            total_resource_fee,
            cpu_insns,
            read_entries,
            write_entries,
            read_bytes,
            tx_size,
            rates,
            DEFAULT_PRECISION,
        );
        let swapped = compute_fee_breakdown(
            total_resource_fee,
            cpu_insns,
            write_entries,
            read_entries,
            tx_size,
            read_bytes,
            swapped_rates,
            DEFAULT_PRECISION,
        );

        prop_assert_eq!(
            original.non_refundable_stroops, swapped.non_refundable_stroops,
            "swapping a component quantity with its rate must not change the non-refundable fee"
        );
        prop_assert_eq!(original.refundable_stroops, swapped.refundable_stroops);
        prop_assert_eq!(original.total_stroops, swapped.total_stroops);
        prop_assert_eq!(original.total_xlm, swapped.total_xlm);
    }
}
