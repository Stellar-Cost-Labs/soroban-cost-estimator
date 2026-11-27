# `estimate`

Simulate a single contract invocation and print the cost report.

## Flags

```
Usage: soroban-cost-estimator estimate [OPTIONS] --wasm <WASM>

Options:
      --format <FORMAT>    Output format: table, json, csv, markdown, or summary [default: table]
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
    "total_stroops": 17122,
    "total_xlm": "0.0017122"
  },
  "ledger_sequence": 3961544,
  "network": "testnet"
}
```

The `ledger_sequence` field records the ledger the simulation ran against,
so consumers can tell how fresh an estimate is. The human-readable header
renders the same value with thousands separators as
`Simulated at ledger sequence: 3,961,544`.

Use `--format json`, `--format csv`, `--format markdown`, or `--format summary`
when feeding the result into CI, spreadsheets, or documentation. `--json`
remains a backward-compatible alias for `--format json`. The result is also
written to the estimate cache (see [Caching](../concepts/caching.md)).

## The `summary` format

`--format summary` prints **exactly one line per successful estimate**, for
shell pipelines, batch logs, and repeated runs where the full table is too
verbose. Nothing else from the report goes to stdout — no header row, no
labels, no fee chart and no advisory sections. Diagnostics (progress,
warnings, cache notices) go to **stderr** for this format, so the result line
can be redirected on its own:

```sh
soroban-cost-estimator estimate --wasm contract.wasm --fn increment \
    --format summary > costs.tsv
```

### Field order

The line has eleven fields, always in this order and always present (an
unknown value is `0`, never an omitted column):

| # | Field | Meaning |
|---|-------|---------|
| 1 | `function` | Contract function, or `(wasm upload)` |
| 2 | `network` | Network the simulation ran against |
| 3 | `ledger` | Ledger sequence simulated against |
| 4 | `total_stroops` | Total fee in stroops (integer) |
| 5 | `total_xlm` | Total fee in XLM, at the current `--precision` |
| 6 | `cpu_instructions` | CPU instructions consumed |
| 7 | `read_entries` | Ledger entries read |
| 8 | `write_entries` | Ledger entries written |
| 9 | `read_bytes` | Ledger bytes read |
| 10 | `write_bytes` | Ledger bytes written |
| 11 | `tx_size` | Transaction envelope size, in XDR bytes |

Fields are separated by a single **tab**, so `cut` and `awk` work without any
custom parsing:

```console
$ soroban-cost-estimator estimate --wasm contract.wasm --fn increment --format summary
increment<TAB>testnet<TAB>3961544<TAB>17122<TAB>0.0017122<TAB>524389<TAB>1<TAB>1<TAB>0<TAB>136<TAB>156
$ soroban-cost-estimator estimate --wasm contract.wasm --fn increment --format summary | cut -f4
17122
```

`<TAB>` marks the delimiter in the transcript above; the tool writes a real
tab character.

### Escaping rules

A field value escapes exactly four characters, with a backslash, using the
C-style convention already used by `ps` and `find -printf`:

| Character in the value | Emitted as |
|---|---|
| backslash (`\`) | `\\` |
| tab (the delimiter) | `\t` |
| line feed | `\n` |
| carriage return | `\r` |

Every other character — spaces, commas, pipes, quotes, non-ASCII text — is
emitted unchanged, so `transfer from` stays readable and a name containing a
`|` or `,` needs no special handling. The escaping guarantees the fixed
column count: splitting the line on tabs always yields exactly eleven fields.

The summary never contains ANSI escape sequences, whether or not stdout is a
terminal, so `> file.tsv` and terminal output are byte-identical. Because the
summary is already minimal, `--quiet` changes nothing about it.

`summary` describes a **single invocation**. `estimate-all` and multi-contract
batch runs (`--wasm` repeated, or `--wasm-dir`) only build aggregates, so they
render their existing **table** output when `summary` is requested; their
`table`, `json`, `csv`, and `markdown` output is unchanged.
