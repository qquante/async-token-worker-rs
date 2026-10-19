# Async Token Worker RS

A Rust asynchronous worker with Redis messaging, bounded concurrency and BSC token-deployment integrations.

![review](https://img.shields.io/badge/review-private-374151) ![primary](https://img.shields.io/badge/primary-Rust-2563EB) [![LinkedIn](https://img.shields.io/badge/LinkedIn-Douglas%20Silva-0A66C2?logo=linkedin&logoColor=white)](https://www.linkedin.com/in/douglas-silva-data-analyst/)

[Overview](#overview) · [Architecture](#architecture) · [Setup](#setup) · [Validation](#validation) · [Roadmap](#roadmap) · [Review notes](docs/REVIEW.md)

## Overview

- Tokio tasks and semaphore limits bound concurrent work.
- Typed serialization and error modules separate message and provider concerns.
- Worker and CLI binaries are controlled through Cargo features.

**Technology:** Rust 2021 · Tokio · reqwest · ethers-rs · Redis · serde · tracing · Docker

**Persistence and data:** Redis Pub/Sub carries jobs/results. Worker state and chain interactions are implemented in Rust modules.

## Architecture

```text
Redis job → typed request → bounded async worker → provider/blockchain adapter → result event
```

| Location | Review focus |
| --- | --- |
| [`Cargo.toml`](Cargo.toml) | Source/configuration inspected for scope and structure |
| [`src/bin/worker.rs`](src/bin/worker.rs) | Source/configuration inspected for scope and structure |
| [`src/api.rs`](src/api.rs) | Source/configuration inspected for scope and structure |
| [`src/blockchain.rs`](src/blockchain.rs) | Source/configuration inspected for scope and structure |
| [`src/broadcaster.rs`](src/broadcaster.rs) | Source/configuration inspected for scope and structure |

Internal package and module names are retained where changing them would affect imports or compatibility. This descriptive repository name does not imply a completed package/API migration.

## Setup

Install Rust/Cargo and inspect feature flags in Cargo.toml. Begin with library inspection or cargo check --lib; do not enable CLI/worker features with a real key as a demonstration.

Start from an isolated environment. Use `.env.example` when present and inspect the configuration loader: an example file is not necessarily loaded automatically. Runtime databases, real account sessions and private credentials are not bundled. Live integration prerequisites are separate from local tests.

## Validation

Static parsing checked 0 Python files and 0 strict JSON files. Python syntax errors: 0; JSON errors: 0. No functional test/build result is claimed for this preparation.

Available test scripts are inventoried in [the review notes](docs/REVIEW.md). Choose offline checks before operating any integration.

These results apply to the selected checks on the preparation copy. They do not establish deployment readiness, full coverage, live provider availability or measured performance. CI definitions, when present under `docs/ci-templates/`, are inactive review templates; no remote CI badge is claimed.

## Limits and current status

This contains signing and real broadcast binaries, including test-fire-real. No Rust toolchain is installed here; compilation, performance and chain behavior are unverified. Do not run those binaries for review.

This private snapshot is for code and portfolio review. Generated runtime material and unreviewed binary assets may be excluded. [Review notes](docs/REVIEW.md) record preparation scope, findings and outstanding validation.

## Roadmap

- [ ] Add a transport abstraction and fully offline worker tests.
- [ ] Require explicit live-execution opt-in with fail-closed defaults.
- [ ] Run cargo check --lib with a controlled toolchain before any integration test.

## History and attribution

No recoverable original Git history was verified for this snapshot. Import and preparation commits use their actual current dates; filesystem timestamps are not treated as development dates.

No open-source license is granted by this preparation. A license and attribution review is required before public redistribution.

## Maintainer and feedback

Douglas de Oliveira Silva · [GitHub](https://github.com/qquante) · [LinkedIn](https://www.linkedin.com/in/douglas-silva-data-analyst/)

For this private review, report reproducible issues with a minimal fixture and the exact check output. Keep account data, credentials and generated sensitive reports out of issues and commits. Existing contribution guidelines apply where present.

**Topics:** `rust`, `tokio`, `redis`, `async`, `blockchain`, `worker`, `docker`
