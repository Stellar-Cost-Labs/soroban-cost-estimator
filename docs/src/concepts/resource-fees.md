# Resource Fees

Every Soroban transaction pays a fee in **stroops** (1 stroop = 10⁻⁷ XLM).
The fee a simulation reports is the *resource fee*: the amount the network
charges for the CPU instructions, ledger I/O, transaction size, and rent your
invocation implies. The RPC returns that total, but the total hides the
structure. This tool splits it into two parts, using the network's own
published rates.

## Non-refundable vs refundable

- **Non-refundable** — the cost of the resources you *consume*: CPU
  instructions, ledger entries read and written, disk bytes read, and the
  transaction's own size. The network keeps this. It is charged regardless of
  what happens after the simulation.
- **Refundable** — the deposit you put down for resources the network *may*
  need later, primarily ledger **rent** (keeping your contract's data alive)
  and contract events. If the data expires early or the rent liability is
  lower than the deposit, part of this comes back.

The split matters for long-running operations: the refundable portion is what
you might get back, so an estimate that lumps it into the flat fee
over-states your true long-term cost.

## How the fee is computed

The authoritative total comes from `simulateTransaction` (the
`minResourceFee` field, which equals the `resource_fee` in the returned
`SorobanTransactionData` XDR). The **non-refundable** portion is derived
*independently* from the network's `ConfigSetting*` rates — never from
hardcoded constants — using integer stroops math, `(units × rate) / scale`,
which preserves precision better than pre-dividing the rate. The refundable
portion is simply the remainder: `total − non_refundable`.

| Component | Formula | Unit |
|-----------|---------|------|
| CPU | `(cpu_insns × fee_rate_per_instructions_increment) / 10_000` | stroops per 10,000 instructions |
| Read entry | `read_entries × fee_disk_read_ledger_entry` | stroops per ledger entry |
| Write entry | `write_entries × fee_write_ledger_entry` | stroops per ledger entry |
| Read bytes | `(read_bytes × fee_disk_read1_kb) / 1024` | stroops per 1 KB |
| Bandwidth | `(tx_size × fee_tx_size1_kb) / 1024` | stroops per 1 KB of tx XDR |

## The real numbers

The cross-check against a live deployed contract (see
[Verification](verification.md)) used `increment(step=5)` on testnet. The
live run reported:

```
CPU instructions: 524,389     Read entries: 1    Write entries: 1
Transaction size: 156 bytes   Write bytes: 136
Fee: non-refundable 4,491 stroops · refundable 12,631 stroops · total 17,122 stroops (0.0017122 XLM)
```

The fee rates come from the config snapshot taken on the same network (the
full snapshot lives in `~/.soroban-cost-estimator/snapshots/` after running
`config snapshot`):

| Rate | Value | Source |
|------|-------|--------|
| `fee_rate_per_instructions_increment` | 7 | `ConfigSettingContractComputeV0` |
| `fee_disk_read_ledger_entry` | 1,563 | `ConfigSettingContractLedgerCostV0` |
| `fee_write_ledger_entry` | 2,500 | `ConfigSettingContractLedgerCostV0` |
| `fee_disk_read1_kb` | 447 | `ConfigSettingContractLedgerCostV0` |
| `fee_tx_size1_kb` | 406 | `ConfigSettingContractBandwidthV0` |

Plug the live run's numbers into the formulas:

```
CPU fee        = (524,389 × 7) / 10,000   = 367 stroops
Read entry fee = 1 × 1,563                = 1,563 stroops
Write entry fee= 1 × 2,500                = 2,500 stroops
Read bytes fee = (0 × 447) / 1,024        = 0 stroops
Bandwidth fee  = (156 × 406) / 1,024      = 61 stroops
Non-refundable = 367 + 1,563 + 2,500 + 61 = 4,491 stroops
Refundable     = 17,122 − 4,491           = 12,631 stroops
```

Both derived values match the report exactly. The refundable portion —
12,631 of the 17,122 stroops, about 74% — is dominated by the ledger rent
deposit for the counter entry the contract writes, which is why this
`increment` contract's *actual* long-term cost is much lower than the
up-front fee.

## When a fee-rate source is missing

All rates come from the network config, never from hardcoded constants. If a
`ConfigSetting*` source cannot be fetched or decoded, the affected rate is
set to 0 **and a warning naming the source is printed** — the non-refundable
fee is then visibly understated rather than silently wrong:

```
Warning: fee rate source(s) ContractComputeV0 unavailable — affected rate(s) set to 0 (non-refundable fee understated)
```

See the [FAQ](faq.md) for what this means in practice.
