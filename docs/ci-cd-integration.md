# CI/CD integration

This guide shows how to run `soroban-cost-estimator` in CI to estimate
contract costs on pull requests and detect network pricing changes on a
schedule. The examples use `testnet`; change `--network` to `mainnet` or
`futurenet` as needed. Estimates simulate against a live Soroban RPC endpoint,
so jobs need network access and results can change when the network changes.

Install the CLI from this repository with `cargo install --path .`, or use
the Docker image described below. The repository's
`tests/fixtures/contract.wasm` makes the examples runnable as written; replace
that path with your compiled contract when adapting them.

## GitHub Actions: comment with pull request cost estimates

Save as `.github/workflows/contract-cost.yml`. Update the WASM path and
invocation arguments for your contract. `--format markdown` produces a
report suitable for a pull request comment; `--id` and `--fn` are required
when estimating an invocation of a deployed contract. Omit them to estimate
the WASM upload instead.

```yaml
name: Contract cost estimate

on:
  pull_request:
    paths:
      - "contracts/**"
      - "tests/fixtures/**"
      - ".github/workflows/contract-cost.yml"

permissions:
  contents: read
  pull-requests: write

jobs:
  estimate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Install estimator
        uses: actions-rs/cargo@v1
        with:
          command: install
          args: --path . --locked

      - name: Estimate contract cost
        run: >-
          soroban-cost-estimator estimate
          --wasm tests/fixtures/contract.wasm
          --network testnet
          --format markdown
          > cost-report.md

      - name: Comment on pull request
        uses: marocchino/sticky-pull-request-comment@v3
        with:
          header: soroban-cost-estimate
          path: cost-report.md
          message: |
            ## Soroban cost estimate

            {{{content}}}
```

The workflow grants the built-in token permission to write pull request
comments. GitHub does not grant write permissions to workflows triggered by
pull requests from forks; for those repositories, keep the comment step
restricted to trusted contributors or publish the estimate as a workflow
artifact instead. Do not check out and execute untrusted pull request code in
a `pull_request_target` workflow.

To fail a pull request on a fee threshold, request JSON output and compare its
`fee.total_stroops` value in a project-specific script. The estimator reports
the measured cost; it does not define a regression baseline or threshold for
your project.

## GitHub Actions: weekly network pricing drift

Save as `.github/workflows/network-drift.yml`. The Actions cache keeps the
snapshot directory between weekly runs. On the first run, the workflow stores
a baseline snapshot. Later runs call `config diff` against that snapshot;
pricing changes return exit code 1 and fail the job. The CLI saves the changed
configuration as the next baseline after detecting pricing drift.

```yaml
name: Network pricing drift

on:
  schedule:
    - cron: "17 8 * * 1"
  workflow_dispatch:

permissions:
  contents: read

jobs:
  drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Install estimator
        uses: actions-rs/cargo@v1
        with:
          command: install
          args: --path . --locked

      - name: Restore network snapshots
        uses: actions/cache/restore@v4
        with:
          path: ~/.soroban-cost-estimator/snapshots
          key: network-snapshots-${{ runner.os }}-${{ github.run_id }}
          restore-keys: |
            network-snapshots-${{ runner.os }}-

      - name: Check pricing drift or create initial baseline
        shell: bash
        run: |
          snapshot_dir="$HOME/.soroban-cost-estimator/snapshots"
          if compgen -G "$snapshot_dir/testnet-*.json" > /dev/null; then
            soroban-cost-estimator config diff --network testnet --summary
          else
            soroban-cost-estimator config snapshot --network testnet --retain 10
          fi

      - name: Save network snapshots
        if: always()
        uses: actions/cache/save@v4
        with:
          path: ~/.soroban-cost-estimator/snapshots
          key: network-snapshots-${{ runner.os }}-${{ github.run_id }}
```

`--summary` prints a compact change count for the job log. To report drift
without failing the workflow, add `--ignore-pricing-exit` to `config diff`.
The cache is scoped to the repository; use a separate cache key if you monitor
multiple networks.

## GitLab CI

Add a `contract-cost` job to `.gitlab-ci.yml`. The example uses the fixture
WASM included in this repository; set `WASM_PATH` to your built contract
artifact in your own project.

```yaml
stages:
  - estimate

contract-cost:
  stage: estimate
  image: rust:1.85
  variables:
    WASM_PATH: "tests/fixtures/contract.wasm"
  before_script:
    - cargo install --path . --locked
  script:
    - soroban-cost-estimator estimate --wasm "$WASM_PATH" --network testnet --format markdown | tee cost-report.md
  artifacts:
    when: always
    paths:
      - cost-report.md
    expire_in: 1 week
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

To fail a job for pricing drift, persist the estimator snapshots as a GitLab
cache and run `soroban-cost-estimator config diff --network testnet` on a
schedule. Seed the cache with `config snapshot --network testnet` before the
first diff, as shown in the GitHub Actions example.

## Docker

Build the CLI image from the repository root with this `Dockerfile`:

```dockerfile
FROM rust:1.85 AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked --bin soroban-cost-estimator

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/soroban-cost-estimator /usr/local/bin/
ENTRYPOINT ["soroban-cost-estimator"]
```

Build and run an upload estimate, mounting the included fixture WASM into the
container. Replace the mounted directory and WASM path with your own build
artifact when adapting the example:

```sh
docker build -t soroban-cost-estimator .
docker run --rm \
  -v "$PWD/tests/fixtures:/contracts:ro" \
  soroban-cost-estimator estimate \
  --wasm /contracts/contract.wasm \
  --network testnet \
  --format markdown
```

For invocation estimates, pass `--id <DEPLOYED_CONTRACT_ID>` and
`--fn <FUNCTION>` followed by any `--arg KEY=VALUE` arguments. To preserve
snapshots or cached estimates across container runs, mount a writable host
directory at `/root/.soroban-cost-estimator`:

```sh
docker run --rm \
  -v "$PWD/.estimator-state:/root/.soroban-cost-estimator" \
  -v "$PWD/contracts/target/wasm32v1-none/release:/contracts:ro" \
  soroban-cost-estimator config snapshot --network testnet
```

The container needs outbound HTTPS access to the selected Soroban RPC
endpoint. Use `--rpc-url` to target a custom endpoint.
