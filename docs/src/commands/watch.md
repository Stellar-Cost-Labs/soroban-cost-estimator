# `watch`

Poll the network's resource-pricing configuration on an interval and print a
diff whenever something changes.

## Flags

```
Usage: soroban-cost-estimator watch [OPTIONS]

Options:
      --network <NETWORK>    Network to watch [default: testnet]
      --interval <INTERVAL>  Polling interval (e.g. "30m", "1h") [default: 1h]
  -h, --help                 Print help
```

## Behavior

- Polls **immediately**, then every `--interval`.
- Intervals accept `s`/`m`/`h`/`d` suffixes or bare seconds: `3600`, `3600s`,
  `30m`, `1h`, `1d`. Unparseable input falls back to one hour.
- Each poll fetches the config, diffs it against the previous snapshot, and
  prints the diff (only when something changed), plus the same stale-cache
  cross-reference as `config diff`.
- **SIGINT (Ctrl-C) and SIGTERM shut it down cleanly** (exit code 0): the
  in-flight poll is cancelled rather than writing a partial snapshot.

## Example

```bash
soroban-cost-estimator watch --network testnet --interval 30m
```

Actual output from a live run (stopped after the first poll with SIGTERM):

```text
Watching testnet for config changes every 3600s... (Ctrl-C to stop)
Received stop signal — exiting cleanly.
```

(The interval prints in seconds: `--interval 1h` → `every 3600s`.)

## Example — poll every 10 minutes in CI

```bash
soroban-cost-estimator watch --network mainnet --interval 10m
```

Run under `systemd`, cron, or a CI job, `watch` gives you a prompt the moment
the network's pricing model changes — and the diff output tells you exactly
which pricing fields moved and which cached estimates are affected.
