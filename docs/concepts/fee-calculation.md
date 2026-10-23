# Fee Calculation Methodology

This page documents, formula by formula, how `soroban-cost-estimator` turns a
`simulateTransaction` response plus the network's `ConfigSetting*` entries into
the fee breakdown in every report. It is the reference for contributors working
on [`src/report/fee_calc.rs`](../../src/report/fee_calc.rs) and for integrators
who need to reproduce a number by hand.

If you only want the intuition, read [Resource Fees](resource-fees.md) first.
This page is the exact arithmetic.

## Where every number comes from

| Value | Source |
| --- | --- |
| Total Soroban resource fee | `simulateTransaction` → `minResourceFee`, or the authoritative `resource_fee` field of the returned `SorobanTransactionData` XDR |
| CPU instructions | `SorobanTransactionData.resources.instructions`, or the older `cost.cpuInsns` |
| Ledger read entries | `SorobanTransactionData.resources.footprint.read_only.length` |
| Ledger write entries | `SorobanTransactionData.resources.footprint.read_write.length` |
| Disk read bytes | `SorobanTransactionData.resources.disk_read_bytes` |
| Write bytes | `SorobanTransactionData.resources.write_bytes` |
| Transaction size | `tx_size` = **XDR byte length** of the envelope the tool built (never the base64 length, which is ~4/3 larger) |
| Fee rates | The network's `CONFIG_SETTING_*` ledger entries (see below) |

Nothing in the fee math is hardcoded except the Stellar **base fee**
(`100` stroops, `BASE_FEE`) and the unit scales (`10,000` instructions,
`1,024` bytes per KiB, `10^7` stroops per XLM). Every rate is read from the
network at simulation time, so a pricing change on the network is visible in
the very next report.

## Config setting structures

Rates are fetched with a single batched `getLedgerEntries` call for the
`LedgerKey::ConfigSetting` keys, then decoded into `stellar_xdr::ConfigSettingEntry`.
Each field used by the fee math belongs to a versioned XDR struct:

| XDR struct | `stellar_xdr` field | Unit | Used for |
| --- | --- | --- | --- |
| `ConfigSettingContractComputeV0` | `fee_rate_per_instructions_increment` | stroops per **10,000** instructions | CPU instruction fee |
| `ConfigSettingContractLedgerCostV0` | `fee_disk_read_ledger_entry` | stroops per ledger **entry** read | Storage read-entry fee |
| `ConfigSettingContractLedgerCostV0` | `fee_write_ledger_entry` | stroops per ledger **entry** written | Storage write-entry fee |
| `ConfigSettingContractLedgerCostV0` | `fee_disk_read1_kb` | stroops per **1 KiB** read from disk | Storage byte read fee |
| `ConfigSettingContractLedgerCostV0` | `rent_fee1_kb_soroban_state_size_low` | stroops per KiB per ledger, below the state target | Rent (inside the refundable portion) |
| `ConfigSettingContractLedgerCostV0` | `rent_fee1_kb_soroban_state_size_high` | stroops per KiB per ledger, above the state target | Rent (inside the refundable portion) |
| `ConfigSettingContractLedgerCostV0` | `soroban_state_target_size_bytes` | bytes | Chooses low/high rent rate |
| `ConfigSettingContractLedgerCostV0` | `soroban_state_rent_fee_growth_factor` | factor | Rent growth once above target |
| `ConfigSettingContractBandwidthV0` | `fee_tx_size1_kb` | stroops per **1 KiB** of transaction size | Bandwidth (transaction size) fee |
| `ConfigSettingContractHistoricalDataV0` | `fee_historical1_kb` | stroops per 1 KiB of historical data | Historic-entry writes, inside the refundable portion |
| `ConfigSettingContractEventsV0` | `fee_contract_events1_kb` | stroops per 1 KiB of emitted events | Events, inside the refundable portion |
| `ConfigSettingStateArchival` | `persistent_rent_rate_denominator` | divisor | Persistent-entry rent |
| `ConfigSettingStateArchival` | `temp_rent_rate_denominator` | divisor | Temporary-entry rent |
| `ConfigSettingStateArchival` | `min_persistent_ttl` / `min_temporary_ttl` / `max_entry_ttl` | ledgers | TTL used by rent |

The tool fetches `ContractComputeV0`, `ContractLedgerCostV0`, and
`ContractBandwidthV0` for the per-resource breakdown, and the same three plus
`StateArchival` for resource-limit warnings and config snapshots.

If a setting cannot be fetched or decoded, its rate is set to `0` **and a
warning naming the source is printed**. A rate is never guessed.

## Symbols

| Symbol | Meaning |
| --- | --- |
| $I$ | CPU instructions consumed |
| $E_r$, $E_w$ | ledger read / write entry counts |
| $B_r$ | disk bytes read |
| $S$ | transaction envelope size in XDR bytes |
| $c$ | `fee_rate_per_instructions_increment` (stroops per 10,000 instructions) |
| $p_r$, $p_w$ | `fee_disk_read_ledger_entry`, `fee_write_ledger_entry` (stroops per entry) |
| $p_{B}$ | `fee_disk_read1_kb` (stroops per KiB) |
| $b$ | `fee_tx_size1_kb` (stroops per KiB) |
| $F$ | total resource fee reported by `simulateTransaction` |
| $F_{\text{base}}$ | base inclusion fee (`100` stroops when $F > 0$) |

All symbols are integers. There is no floating-point arithmetic anywhere in
the fee path — see [Integer arithmetic](#integer-arithmetic).

## The two-tier fee model

$F$ is the **total resource fee** and is taken verbatim from the simulation
response. Per the XDR definition of `SorobanTransactionData.resource_fee`, it
splits into two tiers:

```math
F = F_{\text{non-refundable}} + F_{\text{refundable}}
```

- **Non-refundable** — the cost of resources actually consumed: CPU
  instructions, ledger entry reads and writes, disk bytes read, and the
  transaction's own size.
- **Refundable** — the up-front deposit for resources the network may charge
  for later: ledger **rent** (keeping entries live) plus historical-data and
  contract-event fees. It is refunded to the extent those liabilities turn out
  smaller than the deposit.

The tool computes the non-refundable tier **independently** from the rate
fields above, and derives the refundable tier as the remainder. It never
hardcodes either one.

## Non-refundable formulas

Each component is the number of units times the rate, divided by the unit
scale the rate is quoted in, **floor-divided** (all operands are
non-negative, so floor and truncation coincide):

```math
F_{\text{cpu}} = \left\lfloor \frac{I \cdot c}{10\,000} \right\rfloor
```

```math
F_{\text{read-entry}} = \left\lfloor \frac{E_r \cdot p_r}{1} \right\rfloor
\qquad
F_{\text{write-entry}} = \left\lfloor \frac{E_w \cdot p_w}{1} \right\rfloor
```

```math
F_{\text{read-bytes}} = \left\lfloor \frac{B_r \cdot p_B}{1\,024} \right\rfloor
```

```math
F_{\text{bandwidth}} = \left\lfloor \frac{S \cdot b}{1\,024} \right\rfloor
```

```math
F_{\text{non-refundable}} = F_{\text{cpu}} + F_{\text{read-entry}}
  + F_{\text{write-entry}} + F_{\text{read-bytes}} + F_{\text{bandwidth}}
```

Two details matter for exactness:

1. **The multiplication happens first.** `(units × rate) / scale` preserves
   sub-unit precision; pre-dividing the rate (for example
   `c / 10_000` first) would truncate it to zero and lose the fee entirely.
2. **$S$ is XDR bytes, not base64 characters.** The base64 transport encoding
   is ~33% larger than the binary envelope; using it would overstate the
   bandwidth fee by a third.

Report grouping: the report's `storage_fee_stroops` is
$F_{\text{read-entry}} + F_{\text{write-entry}} + F_{\text{read-bytes}}$, and
`bandwidth_fee_stroops` is $F_{\text{bandwidth}}$, so the four report
components (`cpu_fee_stroops`, `storage_fee_stroops`,
`bandwidth_fee_stroops`, `base_fee_stroops`) plus `refundable_stroops` sum to
`total_stroops`.

## Refundable storage rent, historical data, and events

The refundable tier is not re-derived rate by rate — it is the remainder:

```math
F_{\text{refundable}} = \max\left(0, \; F - F_{\text{non-refundable}}\right)
```

The `max(0, …)` is a defensive floor for one legitimate edge case: a response
that omits the fee (callers then pass $F = 0$) while the config rates still
yield a positive non-refundable cost. Without the floor the refundable value
would go negative, which is impossible on the network. `saturating_sub` alone
is not enough there because it saturates at `i64::MIN`, not at zero.

Inside that remainder sits the **rent deposit**. Stellar Core sizes it per
footprint entry from the entry's byte size, its remaining TTL, and the rent
rate:

```math
\text{rent}_{\text{entry}} = \left\lceil
  \frac{\text{size}_{\text{bytes}} \cdot r \cdot
        \left(\text{live\_until} - \text{current\_ledger}\right)}
       {1\,024 \cdot d}
\right\rceil
```

where:

- $r$ is `rent_fee1_kb_soroban_state_size_low` while the live Soroban state is
  at or below `soroban_state_target_size_bytes`, and
  `rent_fee1_kb_soroban_state_size_high` above it (scaled by
  `soroban_state_rent_fee_growth_factor` as the state grows further);
- $d$ is `persistent_rent_rate_denominator` for persistent entries and
  `temp_rent_rate_denominator` for temporary ones;
- `live_until` is the entry's expiration ledger, at least `min_persistent_ttl`
  / `min_temporary_ttl` beyond the current ledger and at most `max_entry_ttl`.

The transaction also pre-pays `fee_historical1_kb` for any archival
(`BUCKET_LIST`) entries it writes and `fee_contract_events1_kb` for the event
bytes it emits; both are part of the refundable deposit.

**Why the tool does not recompute rent:** only the network knows each
footprint entry's live TTL and current size, and the simulation has already
priced them. Re-deriving the sum would duplicate Core's state-dependent logic
and could disagree with the network. Taking $F$ as the authority and splitting
it keeps the refundable figure exact by construction, while the
non-refundable tier — the part that is never returned — is still broken out
with the network's own published rates.

## Base fee and total

```math
F_{\text{base}} =
\begin{cases}
100 & F > 0 \\
0 & F = 0
\end{cases}
```

```math
F_{\text{total}} = F + F_{\text{base}}
```

`100` stroops is Stellar's protocol base fee per operation; a simulation that
reported no resource fee at all (for example an upload-only dry run with no
data) is not charged one, which is why the base fee is conditional rather than
unconditional.

## XLM conversion

Stroops are the unit of account in every calculation; XLM is presentation only:

```math
\text{XLM} = \frac{F_{\text{total}}}{10^{7}}
```

The conversion is integer-exact and rounds **half-up** at the requested
`--precision` (0–7 decimal places, default 7):

```text
stroops_to_xlm(1_234_567, 7) = "0.1234567"
stroops_to_xlm(1_234_567, 4) = "0.1235"    # half-up on the dropped digits
stroops_to_xlm(9_999_999, 6) = "1.000000"  # rounding carries into the whole
```

Floating point is never used for an amount, so a fee round-trips as exactly
the stroop count the network reported.

## Percentages

`fee_percentages` sums to **exactly 100.0%** rather than approximately. Each
component is converted to per-mille with half-up integer rounding,

```math
\text{permille}_i = \operatorname{round}\!\left(\frac{1000 \cdot x_i}{F_{\text{total}}}\right)
```

then the rounding residue is applied to the largest component:

```math
\text{permille}_{\max} \mathrel{+}= 1000 - \sum_i \text{permille}_i
```

so no percentage is ever displayed that misattributes the rounding error.

## Integer arithmetic

- **Types.** Stroops are `i64`; unit counts are `u64`/`u32`. Fees never touch
  `f32`/`f64`.
- **Wide intermediates.** `scaled_fee` performs `units × rate` in `i128`, so
  `u64::MAX × i64::MAX` cannot wrap, and clamps the quotient into `i64`:
  `i64::MAX` for a positive overflow, `i64::MIN` for a negative one.
- **No silent clamping of the non-refundable tier.** The derived
  non-refundable total is *not* capped at $F$. Capping would hide a genuine
  bug (wrong transaction size, wrong rate lookup) behind a plausible-looking
  number. When the inputs really do disagree, both values are reported and the
  refundable tier floors at zero.
- **Saturation.** Component sums use `saturating_add`, so an adversarial or
  corrupt config cannot panic the fee path.
- **Idempotence.** The computation is pure: the same
  `(F, I, E_r, E_w, B_r, S, rates, precision)` always yields the same
  breakdown, which is what makes the estimate cache and the snapshot tests
  sound.

## Worked example (testnet, `increment(step=5)`)

Rates from a `config snapshot` taken on the same network:

| Rate | Value |
| --- | --- |
| $c$ (`fee_rate_per_instructions_increment`) | 7 |
| $p_r$ (`fee_disk_read_ledger_entry`) | 1,563 |
| $p_w$ (`fee_write_ledger_entry`) | 2,500 |
| $p_B$ (`fee_disk_read1_kb`) | 447 |
| $b$ (`fee_tx_size1_kb`) | 406 |

Simulation: $I = 524{,}389$, $E_r = 1$, $E_w = 1$, $B_r = 0$, $S = 156$,
$F = 17{,}122$.

```text
F_cpu         = (524389 × 7) / 10000        = 367
F_read-entry  = 1 × 1563                    = 1563
F_write-entry = 1 × 2500                    = 2500
F_read-bytes  = (0 × 447) / 1024            = 0
F_bandwidth   = (156 × 406) / 1024          = 61
F_non-refundable = 367 + 1563 + 2500 + 0 + 61 = 4491
F_refundable     = 17122 − 4491               = 12631
F_base           = 100
F_total          = 17122 + 100                = 17222 (0.0017222 XLM)
```

The report renders `non_refundable_stroops: 4491`,
`refundable_stroops: 12631`, `base_fee_stroops: 100` and
`total_stroops: 17222`. The two split tiers are exactly the values
[Resource Fees](resource-fees.md) derives by hand from the same $F = 17{,}122$
resource fee; `total_stroops` adds the 100-stroop base inclusion fee that the
transaction also pays. Note that the refundable tier (~73% here) is the ledger
rent deposit for the counter entry the contract writes, so the invocation's
long-term cost is far below its up-front fee.

## Why the estimates match Stellar Core

`simulateTransaction` is executed by Stellar Core itself, and the tool copies
the fee total from that response rather than recomputing it:

1. **The total is authoritative.** $F$ is `resource_fee` from the returned
   `SorobanTransactionData` (or the identical `minResourceFee`). It is Core's
   own number, so no deviation is possible for the total.
2. **The split uses Core's inputs.** The non-refundable tier is derived from
   the same `ConfigSetting*` entries Core prices against — fetched from the
   ledger, not constants compiled into the tool.
3. **The scaling matches.** `(units × rate) / scale` with floor division and
   wide intermediates is exactly Core's resource-fee arithmetic; nothing is
   rounded twice.
4. **The units match.** Entry counts come from the simulation's footprint,
   bytes from `disk_read_bytes`, and transaction size from the XDR envelope the
   tool submitted — the same envelope the simulation priced.
5. **The derived split is self-checking.** If the config-derived non-refundable
   tier ever exceeded $F$, the report would show both numbers rather than
   hide the discrepancy (see "No silent clamping" above).

That is also why `--precision` cannot change fees: it only changes how the
integer stroop total is rendered as XLM.

## Reproducing a report offline

Given a cached estimate and a config snapshot, all of the above is
recomputable with integer arithmetic only:

```sh
soroban-cost-estimator config snapshot --network testnet   # capture the rates
soroban-cost-estimator estimate --wasm contract.wasm --fn increment --format json
```

The JSON carries `cpu_instructions`, `read_entries`, `write_entries`,
`read_bytes`, `write_bytes`, `tx_size`, and the whole `fee` object, so the
formulas on this page can be checked column by column.

## Related reading

- [Resource Fees](resource-fees.md) — the concepts, without the arithmetic
- [Config Drift](config-drift.md) — how the rates above change over time
- [Caching](caching.md) — how past estimates and their rates are retained
- [Verification](../verification.md) — the live cross-check these formulas are
  validated against
