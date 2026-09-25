# `wasm info`

Print everything about a compiled Soroban contract that can be derived from
the `.wasm` file itself: size, hash, exported function signatures, embedded
metadata, and a section size summary. The command performs **zero network
access** — it only reads and parses the file.

## Flags

```
Usage: soroban-cost-estimator wasm info [OPTIONS] <WASM>

Arguments:
  <WASM>  Path to the compiled Soroban contract `.wasm` file

Options:
      --json  Output the full parsed spec/metadata as JSON
  -h, --help  Print help
```

The older flat form `soroban-cost-estimator wasm-info --wasm <WASM> [--json]`
is still supported and produces the same report.

## Behavior

- Validates the binary before parsing it, and exits with code 1 plus a
  `not a valid WebAssembly binary` error when validation fails.
- Prints the raw file size and the hex SHA-256 digest of the file bytes.
- Lists every exported function with the argument and return types decoded
  from the `contractspecv0` contract spec, e.g. `increment(step: i64) -> i64`.
  Without a spec section, only the bare WASM export names are shown.
- Reports the contract metadata from the `contractmetav0` section: name,
  version, description, custom keys, and the Soroban SDK version
  (`rssdkver`) the contract was built with.
- Prints a section size breakdown table: every section (code, data, type,
  function, export, import, …) and custom sections such as `contractspecv0`,
  `contractmetav0`, `name`, and `producers`, each with its byte size and its
  share of the file, largest first. See
  [Section size accounting](#section-size-accounting).
- With `--json`, emits the full parsed AST: all `contractspecv0` entries
  (functions, UDT structs, unions, enums, error enums, events) with their
  nested type trees, plus the module's imports, exports, and memory limits.

## Example

```bash
soroban-cost-estimator wasm info tests/fixtures/contract.wasm
```

Actual output:

```text
WASM info: tests/fixtures/contract.wasm
  Size:      4742 bytes
  SHA-256:   ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd
  Functions: 1
    [1] increment(step: i64) -> i64
  Contract spec: present (1 entries, typed params/returns decoded from contractspecv0)
  SDK version:   25.3.2
WASM sections: 15 section(s), 4734 bytes of section data in a 4742 byte file (8 byte module header)
  section            bytes  share
  name               3047   64.3%
  code               1021   21.5%
  contractspecv0      150    3.2%
  contractmetav0      113    2.4%
  type                 83    1.8%
  producers            79    1.7%
  export               55    1.2%
  target_features      36    0.8%
  global               35    0.7%
  import               33    0.7%
  contractenvmetav0    32    0.7%
  function             27    0.6%
  data                 11    0.2%
  module header         8    0.2%
  table                  7    0.1%
  memory                 5    0.1%
Contract meta: present
  rsver: 1.96.0
  rssdkver: 25.3.2
```

WASM size drives upload fees and rent, so the breakdown makes it obvious
where the bytes go: in the example above, the `name` custom section alone
accounts for 64.3% of the file.

## Section size accounting

A WASM module is the 8-byte magic + version header followed by sections, and
nothing else — so this breakdown accounts for every byte of the file:

- Each row's size is the section's **full footprint**: its contents plus its
  own header (the section id byte and its LEB128 size prefix).
- The fixed 8-byte module header is reported as its own `module header` row.
- Section bytes + `module header` therefore add up **exactly** to the file
  length, and the shares add up to 100%.
- Sections are listed largest first. Byte offsets and raw content sizes for
  each section are available in the `sections` array of `--json` output.


## JSON output

```bash
soroban-cost-estimator wasm info tests/fixtures/contract.wasm --json
```

The document has these top-level keys:

| Key | Contents |
|-----|----------|
| `path` | Path exactly as given on the command line |
| `size` | File size in bytes |
| `sha256` | Hex SHA-256 digest of the file bytes |
| `has_spec` | Whether a `contractspecv0` section was found |
| `sdk_version` | Soroban SDK version from `contractmetav0`, or `null` |
| `section_sizes` | Map of section name → bytes (header included), including the `module header` row; the values sum to `size` |
| `sections` | One object per row, largest first: `id`, `name`, `custom`, `offset`, `end`, `size` (contents), `header_size`, `total_size`, `percent` |
| `contract_meta` | `name`, `version`, `description`, `sdk_version`, and the full ordered `entries` list |
| `functions` | Exported functions with `param_count`, `result_count`, `signature`, `params`, and `returns` |
| `spec_entries` | Every decoded `contractspecv0` entry (`kind`, `name`, `doc`, plus kind-specific fields) |
| `module` | `start_function`, `memories`, `imports`, and `exports` |

Spec type definitions are rendered as structured JSON so nested types keep
their shape:

```json
{ "type": "option", "inner": { "type": "vec", "element": { "type": "i64" } } }
```

Pipe the output through any JSON tool, for example:

```bash
soroban-cost-estimator wasm info contract.wasm --json | jq '.spec_entries[].name'
soroban-cost-estimator wasm info contract.wasm --json | jq '.section_sizes'
```

```json
{
  "code": 1021,
  "contractmetav0": 113,
  "contractspecv0": 150,
  "data": 11,
  "export": 55,
  "function": 27,
  "global": 35,
  "import": 33,
  "memory": 5,
  "module header": 8,
  "name": 3047,
  "producers": 79,
  "table": 7,
  "target_features": 36
}
```

## Section breakdown in cost reports

`estimate` prints the same breakdown for the contract it just simulated, right
after the resource table, so a fee estimate shows what is being uploaded:

```text
WASM size: 4742 bytes
WASM hash: ea14bca998e98f0…
…
WASM sections (4734 bytes of section data, largest first):
+-------------------+-------+-------+
| Section           | Bytes | Share |
+===================================+
| name              | 3047  | 64.3% |
| code              | 1021  | 21.5% |
| contractspecv0    | 150   | 3.2%  |
| …                 |       |       |
+-------------------+-------+-------+
  4742 bytes accounted for (100.0% of the file)
```

`estimate --format json` carries the same data as `wasm_size`, `wasm_sections`
(an array with `percent` per row), and a `section_sizes` map.
