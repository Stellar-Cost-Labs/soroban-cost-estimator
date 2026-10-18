# CI/CD Integration Guide

Automating Soroban smart contract cost estimation and network configuration drift detection in your CI/CD pipelines helps catch fee regressions before deployment and alerts your team when Stellar governance updates network pricing parameters.

---

## 1. GitHub Actions: PR Cost Estimation & Budget Gate Comment

Create `.github/workflows/soroban-cost-check.yml` to build your contract WASM on every Pull Request, enforce resource budget gates, and post a Markdown cost report directly on the PR:

```yaml
name: Soroban Contract Cost Check

on:
  pull_request:
    branches: [main]
    paths:
      - "contracts/**"
      - "Cargo.toml"
      - "Cargo.lock"

permissions:
  contents: read
  pull-requests: write

jobs:
  estimate-cost:
    name: Estimate Soroban Resource Costs
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown

      - name: Cache Cargo registry and build artifacts
        uses: Swatinem/rust-cache@v2

      - name: Install soroban-cost-estimator
        run: cargo install soroban-cost-estimator --locked

      - name: Build contract WASM
        run: cargo build --target wasm32-unknown-unknown --release

      - name: Run cost estimation and generate Markdown report
        id: estimate
        env:
          CONTRACT_WASM: target/wasm32-unknown-unknown/release/my_contract.wasm
        run: |
          soroban-cost-estimator estimate \
            --wasm "$CONTRACT_WASM" \
            --network testnet \
            --format markdown \
            --quiet > cost-report.md

      - name: Comment cost report on Pull Request
        uses: marocchino/sticky-pull-request-comment@v2
        with:
          header: soroban-cost-report
          path: cost-report.md
```

### Side-by-Side Diff Against `main` Branch WASM

To compare the PR's compiled `.wasm` against the baseline artifact from `main`:

```bash
soroban-cost-estimator estimate \
  --wasm baseline-contract.wasm \
  --wasm-new target/wasm32-unknown-unknown/release/my_contract.wasm \
  --diff \
  --network testnet
```

---

## 2. GitHub Actions: Scheduled Weekly Network Pricing Drift Detection

Create `.github/workflows/soroban-config-drift.yml` to check whether the Stellar network's on-chain `CONFIG_SETTING` fee rates have changed since your committed baseline snapshot. By default, `config diff` exits with code `1` when any pricing parameter changes:

```yaml
name: Soroban Network Pricing Drift Monitor

on:
  schedule:
    # Run every Monday at 08:00 UTC
    - cron: "0 8 * * 1"
  workflow_dispatch:

jobs:
  check-config-drift:
    name: Detect Network Fee Config Drift
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install soroban-cost-estimator
        run: cargo install soroban-cost-estimator --locked

      - name: Import baseline snapshots and diff against live network
        env:
          SOROBAN_RPC_TOKEN: ${{ secrets.SOROBAN_RPC_TOKEN }}
        run: |
          # Import committed baseline snapshot bundle
          soroban-cost-estimator config import --input ci/snapshots-mainnet.json

          # Fetch latest network configsnapshot
          soroban-cost-estimator config snapshot --network mainnet

          # Exit code 1 if any pricing change >= 5% is detected
          soroban-cost-estimator config diff \
            --network mainnet \
            --threshold 5.0
```

### CI Exit Code Reference for `config diff`

| Flag Combination | Exit Code `0` | Exit Code `1` |
|---|---|---|
| Default (`config diff`) | No pricing changes | At least one pricing field changed |
| `--threshold <PCT>` | All pricing changes `< PCT%` | Any pricing field changed by `>= PCT%` |
| `--fail-on-any-change` | Snapshots are identical | Any config field changed (pricing or non-pricing) |
| `--ignore-pricing-exit` | Always exits `0` (informational report) | Never (unless RPC/parse error) |

---

## 3. GitLab CI Integration (`.gitlab-ci.yml`)

Add the following job to your `.gitlab-ci.yml` pipeline to run contract cost estimation and archive the JSON report as a build artifact:

```yaml
stages:
  - build
  - cost-check

soroban_cost_estimation:
  stage: cost-check
  image: rust:1.85-bookworm
  variables:
    CARGO_HOME: "$CI_PROJECT_DIR/.cargo"
  cache:
    key: "$CI_COMMIT_REF_SLUG"
    paths:
      - .cargo/bin/
      - .cargo/registry/
      - target/
  before_script:
    - rustup target add wasm32-unknown-unknown
    - command -v soroban-cost-estimator || cargo install soroban-cost-estimator --locked
  script:
    - cargo build --target wasm32-unknown-unknown --release
    - soroban-cost-estimator estimate
        --wasm target/wasm32-unknown-unknown/release/my_contract.wasm
        --network testnet
        --json > cost-report.json
    - soroban-cost-estimator estimate
        --wasm target/wasm32-unknown-unknown/release/my_contract.wasm
        --network testnet
        --quiet
  artifacts:
    when: always
    paths:
      - cost-report.json
    expire_in: 30 days
```

---

## 4. Docker Usage Guide

You can package `soroban-cost-estimator` into a lightweight container image for reproducible CI execution across any runner environment (Jenkins, CircleCI, Buildkite, Kubernetes):

```dockerfile
FROM rust:1.85-slim-bookworm AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /usr/src/soroban-cost-estimator
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/soroban-cost-estimator/target/release/soroban-cost-estimator /usr/local/bin/soroban-cost-estimator
ENTRYPOINT ["soroban-cost-estimator"]
```

Build and invoke the container mounting your local workspace:

```bash
# Build the Docker image
docker build -t soroban-cost-estimator:latest .

# Estimate a compiled WASM file from the host filesystem
docker run --rm \
  -v "$(pwd):/workspace" \
  soroban-cost-estimator:latest \
  estimate --wasm /workspace/contract.wasm --network testnet --json
```
