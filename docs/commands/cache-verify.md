# `cache verify`

Check that every cached estimate in
`~/.soroban-cost-estimator/cache/` is valid JSON and parses as a cache entry —
i.e. nothing was corrupted by a crash or disk issue.

## Flags

```
Usage: soroban-cost-estimator cache verify [OPTIONS]

Options:
      --help  Print help
```

No additional flags — this is a pure file I/O operation with no network calls.

## Behavior

- Reads every file in the cache directory and attempts to parse it as valid
  `CachedEstimate` JSON.
- Prints a summary line per corrupted entry (filename).
- **Exit code 0** if the cache is empty or every entry is valid.
- **Exit code 1** and lists corrupted filenames if any entry fails.
- Scripts and CI can treat a corrupt cache as an error condition.
- No network calls are made — this is pure file I/O.

## Example

```bash
soroban-cost-estimator cache verify
```

Actual output (healthy cache):

```text
Checked 5 cache entries.
All cache entries are valid.
```

Actual output (corrupt cache):

```text
Checked 5 cache entries.
2 of 5 cache entries failed verification:
  - abc123_increment_step5.json
  - def456_upload_.json
```

Empty cache:

```text
Cache is empty — nothing to verify.
```
