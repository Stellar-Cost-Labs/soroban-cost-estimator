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
- Summarizes every WASM section with its name, id, and byte size, including
  custom sections such as `contractspecv0`, `contractmetav0`, `name`, and
  `producers`.
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
Sections: 15 (4701 bytes of section content in a 4742 byte file)
  [1] type (id 1): 81 bytes
  [2] import (id 2): 31 bytes
  [3] function (id 3): 25 bytes
  [4] table (id 4): 5 bytes
  [5] memory (id 5): 3 bytes
  [6] global (id 6): 33 bytes
  [7] export (id 7): 53 bytes
  [8] code (id 10): 1018 bytes
  [9] data (id 11): 9 bytes
  [10] contractspecv0 (id 0): 147 bytes
  [11] contractenvmetav0 (id 0): 30 bytes
  [12] contractmetav0 (id 0): 111 bytes
  [13] name (id 0): 3044 bytes
  [14] producers (id 0): 77 bytes
  [15] target_features (id 0): 34 bytes
Contract meta: present
  rsver: 1.96.0
  rssdkver: 25.3.2
```

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
| `sections` | One object per section: `id`, `name`, `custom`, `offset`, `end`, `size` |
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
```
