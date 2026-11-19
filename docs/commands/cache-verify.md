# `cache verify`

Check that every cached estimate in
`~/.soroban-cost-estimator/cache/` is valid JSON and parses as a cache entry —
i.e. nothing was corrupted by a crash or disk issue.

## Flags

```
Usage: soroban-cost-estimator cache verify [OPTIONS]

Options:
      --repair  Delete corrupted cache entries instead of only reporting them [aliases: --fix]
      --help    Print help
```

No network calls are made.

## Behavior

- Reads every file in the cache directory and attempts to parse it as valid
  `CachedEstimate` JSON.
- Prints one `Corrupted cache entry: <name>` line per corrupted entry and a
  `Verified X cache files: Y valid, Z corrupted` summary.
- Entries written by a newer schema count as corrupted, because this build
  cannot read them.
- With `--repair` (or `--fix`), deletes the corrupted entries and keeps the
  valid ones.
- **Exit code 0** if the cache is empty, every entry is valid, or `--repair`
  removed the corrupted entries.
- **Exit code 1** if any entry fails and `--repair` was not given.
- Scripts and CI can treat a corrupt cache as an error condition.
- No network calls are made — this is pure file I/O.

## Example

```bash
soroban-cost-estimator cache verify
```

Actual output (healthy cache):

```text
Verified 5 cache files: 5 valid, 0 corrupted
```

Actual output (corrupt cache):

```text
Corrupted cache entry: abc123-increment-9f86d0….json
Corrupted cache entry: def456-upload-e3b0c4….json
Verified 5 cache files: 3 valid, 2 corrupted
Run `cache verify --repair` to remove the corrupted files.
```

Repairing it:

```bash
soroban-cost-estimator cache verify --repair
```

```text
Corrupted cache entry: abc123-increment-9f86d0….json
Corrupted cache entry: def456-upload-e3b0c4….json
Removed 2 corrupted cache file(s).
Verified 5 cache files: 3 valid, 2 corrupted (removed)
```

Empty cache:

```text
Cache is empty — nothing to verify.
```
