# Async Token Worker RS

A Rust asynchronous worker with Redis messaging, bounded concurrency and BSC token-deployment integrations.

## Features

- Tokio tasks and semaphore limits bound concurrent work.
- Typed serialization and error modules separate message and provider concerns.
- Worker and CLI binaries are controlled through Cargo features.

## Stack

Rust 2021 · Tokio · reqwest · ethers-rs · Redis · serde · tracing · Docker

Redis Pub/Sub carries jobs/results. Worker state and chain interactions are implemented in Rust modules.

## Getting started

Install Rust/Cargo and inspect feature flags in Cargo.toml. Begin with library inspection or cargo check --lib; do not enable CLI/worker features with a real key as a demonstration.

## How it works

```text
Redis job → typed request → bounded async worker → provider/blockchain adapter → result event
```

## Current limitations

This contains signing and real broadcast binaries, including test-fire-real. No Rust toolchain is installed here; compilation, performance and chain behavior are unverified. Do not run those binaries for review.

## License

No open-source license has been granted. Redistribution terms remain to be defined.

## Provider configuration

Optional provider credentials are read from the environment variables listed in `.env.example`. Providers with missing or blank values are omitted. Never commit a populated environment file. No credentials are bundled. Historical credentials must be rotated at their providers; deleting them from Git does not invalidate them.
