# `estimate`

Simulate a single contract invocation and print the cost report.

## Flags

```
Usage: soroban-cost-estimator estimate [OPTIONS] --wasm <WASM>

Options:
      --format <FORMAT>    Output format: table, json, csv, or markdown [default: table]
  -w, --wasm <WASM>        Path to the compiled Soroban contract `.wasm` file
      --network <NETWORK>  Network to simulate against [default: testnet]
      --rpc-url <RPC_URL>  Explicit RPC URL (overrides network-based resolution)
      --fn <FN>            Contract function name to invoke
      --id <ID>            Deployed contract ID (64 hex chars) to invoke. Required when --fn is used
      --arg <KEY=VAL>      Function arguments as key=value pairs (value is type-inferred)
  -i, --interactive        Prompt for the function and its arguments using the contract spec
      --json               Output as JSON instead of a human-readable table
  -h, --help               Print help
```

`--wasm` is required. Everything else is optional.

## Behavior

- **Without `--fn`**, the tool simulates *uploading* the contract WASM to the
  network. The report's function is `(wasm upload)`.
- **With `--fn`**, it simulates invoking a specific contract function against
  a **deployed** contract, so `--id <64-hex>` is required —
  `simulateTransaction` loads the contract instance from the ledger and
  cannot simulate against a zeroed ID.
- **`--arg` values are type-inferred**: `true`/`false` → bool, integers →
  i64/u64, everything else → string. Enough for cost estimation.
- **`--interactive` / `-i`** prompts for the function and its arguments
  instead of requiring `--fn`/`--arg` up front. The contract spec provides
  names and types (`Enter step (i64): `); each answer is validated against
  its declared type before proceeding, with re-prompts on mismatch.
  Explicit `--fn`/`--arg`/`--id` values take precedence — the prompt only
  fills in the gaps, so a fully-specified invocation never blocks on stdin.
  EOF (Ctrl-D) aborts with a clear error; Ctrl-C aborts without panicking.
- The read/write entry counts and byte sizes are decoded from the simulation
  response's resource **footprint** — real values from the ledger footprint,
  not zero-filled placeholders.
- If a fee-rate source (`ConfigSetting*`) can't be fetched, a warning naming
  the source is printed and only the affected rate is zeroed, so the
  non-refundable fee is visibly understated rather than silently wrong.
- A simulation that returns no cost data and no latest ledger fails loudly
  with an error naming `--id`, `--fn`, and the RPC endpoint — it is treated
  as a misconfigured request, not a free transaction.

## Example — upload simulation

```bash
soroban-cost-estimator estimate --wasm tests/fixtures/contract.wasm --network testnet
```

## Example — invoke a function against a deployed contract

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --fn increment --arg step=5
```

## Example — interactive argument entry

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --interactive
```

```text
Available functions:
  1. increment(step: i64)
Select a function [1-1 or name]: 1
Enter step (i64): 5
```

Omit `--id` and it is prompted for too (validated as 64-hex or `C…`
strkey). Pass `--fn` and/or `--arg` alongside `-i` to skip those prompts.

This is the exact invocation cross-checked against the native Stellar CLI
(see [Verification](../verification.md)). Actual output from a live testnet run:

```text
Function: increment
Network: testnet (ledger 3961551)
Simulated at ledger sequence: 3,961,551
WASM hash: ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd

+------------------+----------+---------------+
| Resource         | Consumed | Fee (stroops) |
+=============================================+
| CPU Instructions | 524389   |               |
|------------------+----------+---------------+
| Memory Bytes     | 0        |               |
|------------------+----------+---------------+
| Read Entries     | 1        |               |
|------------------+----------+---------------+
| Write Entries    | 1        |               |
|------------------+----------+---------------+
| Read Bytes       | 0        |               |
|------------------+----------+---------------+
| Write Bytes      | 136      |               |
|------------------+----------+---------------+
| Transaction Size | 156      |               |
+------------------+----------+---------------+

Fee Breakdown:
  Non-refundable: 4491 stroops
  Refundable:     12631 stroops
  Total:          17122 stroops (0.0017122)
```

The fee column in the resource table is intentionally blank: the per-resource
fee is embedded in the non-refundable total, which is derived from the
network's own config rates. See [Resource Fees](../concepts/resource-fees.md)
for the traced math.

## Example — machine-readable output

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet --fn increment --arg step=5 --json
```

```json
{
  "function": "increment",
  "wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "cpu_instructions": 524389,
  "memory_bytes": 0,
  "tx_size": 156,
  "read_entries": 1,
  "write_entries": 1,
  "read_bytes": 0,
  "write_bytes": 136,
  "fee": {
    "non_refundable_stroops": 4491,
    "refundable_stroops": 12631,
    "cpu_fee_stroops": 360,
    "storage_fee_stroops": 4020,
    "bandwidth_fee_stroops": 11,
    "base_fee_stroops": 100,
    "total_stroops": 17122,
    "total_xlm": "0.0017122",
    "fee_percentages": {}
  },
  "ledger_sequence": 3961544,
  "network": "testnet",
  "rpc_latency_ms": 84,
  "warnings": [],
  "contract_meta": {},
  "suggestions": [],
  "optimization_suggestions": []
}
```

(The example above is elided: real output also includes `wasm_size`, and
optional keys like `history` appear when requested. The key sets and their
compatibility guarantees are specified in the next section.)

The `ledger_sequence` field records the ledger the simulation ran against,
so consumers can tell how fresh an estimate is. The human-readable header
renders the same value with thousands separators as
`Simulated at ledger sequence: 3,961,544`.

Use `--format json`, `--format csv`, or `--format markdown` when feeding the result into CI, spreadsheets, or documentation. `--json` remains a backward-compatible alias for `--format json`. The
result is also written to the estimate cache(see [Caching](../concepts/caching.md)).

## JSON output compatibility

The JSON emitted by this tool is a public, machine-consumed interface. The
shapes below are locked by automated schema tests (see the note at the end
of this section); breaking them requires going through the compatibility
policy.

### Documented families

| Command | JSON document | Root keys |
| --- | --- | --- |
| `estimate --json` | single cost report | `function`, `wasm_hash`, `cpu_instructions`, `memory_bytes`, `tx_size`, `read_entries`, `write_entries`, `read_bytes`, `write_bytes`, `fee`, `ledger_sequence`, `network`, `rpc_latency_ms`, `warnings`, `contract_meta`, plus the derived `suggestions` and `optimization_suggestions` |
| `estimate-all --json` | batch document | `functions` (array) and `fee_distribution` (integer statistics object) |
| batch contract summaries | multi-contract report (`estimate` with repeated `--wasm` or `--wasm-dir`) | `network`, `contracts` (array), `total_contracts`, `succeeded`, `failed` |
| `estimate --diff --json` | side-by-side diff | `identity`, `rows`; optional `abi_diff` |
| `wasm info --json` | WASM inspection | `path`, `size`, `sha256`, `has_spec`, `sdk_version`, `has_debug_symbols`, `debug_symbol_bytes`, `estimated_size_reduction_percent`, `contract_meta`, `functions`, `spec_entries`, `section_sizes`, `sections`, `imports`, `host_imports`, `module` |

The `fee` object always carries the full breakdown:
`non_refundable_stroops`, `refundable_stroops`, `cpu_fee_stroops`,
`storage_fee_stroops`, `bandwidth_fee_stroops`, `base_fee_stroops`,
`total_stroops` (all integer stroops), `total_xlm` (a string, to avoid
float precision loss) and `fee_percentages`.

### Absent versus null

Optional data leaves a document in exactly one of two documented ways:

* **Omitted.** Fields declared with `skip_serializing_if` disappear from the
  object when unset — e.g. `wasm_size`, `wasm_sections`, `section_sizes`,
  `history`, `projections` on a single estimate, the `reason`/`error` and
  resource blocks on `estimate-all` entries, and empty `abi_diff`
  collections. Absence means "not measured / not requested", never zero.
* **Explicit `null`.** Keys built inside `json!` documents (the `wasm info`
  family) or fields without a skip predicate — e.g.
  `contract_meta.name`, `module.start_function`, `max_pages` on the WASM
  structure summary and `rows[].change_percent` in a diff — are always
  present and carry `null` when there is no value. `change_percent` is
  `null` when the baseline was zero and the new value is not, i.e. the
  percentage is mathematically unbounded.

Consumers should treat both forms as "no value" and must not assume that
key presence alone distinguishes them beyond what is documented above.

### Compatibility policy

* **Additive, optional keys are non-breaking.** New fields may appear in any
  JSON document in a minor or patch release. Consumers must ignore unknown
  keys (parse tolerantly) rather than failing on unexpected members.
* **Renaming, removing, retyping, or re-semanticizing a documented key is
  breaking.** That includes flipping a field between "omitted" and "null",
  changing the string set of a lowercase enum (`status`, `trend`,
  `direction`), or folding a new fee component into `fee.total_stroops`.
  Breaking changes land only in a major release and are recorded in
  [Migration](../migration.md).
* **Key order is never contractual.** Objects may be emitted in any order;
  do not snapshot raw text as an API dependency.
* **Fee math stays integer.** All stroop amounts are JSON integers derived
  by integer arithmetic; XLM totals are strings. Floats appear only in
  display-oriented fields such as `change_percent` and `percent`.
* **The contract is executable.** Structural tests live in
  `tests/json_schema_tests.rs` (single estimate, batch summaries, diff,
  WASM structure summary) and in the `src/main.rs` test module
  (`estimate-all` document and `wasm info` document). Any change that
  fails one of these tests is, by definition, a compatibility decision —
  update this document, the tests, and the migration notes together.
