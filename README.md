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

This contains signing and real broadcast binaries, including test-fire-real. Chain behavior and performance are unverified; the validation below does not execute those binaries.

## Validation

Rust 1.99.1 on Windows/MSVC: library compilation, three library tests and compilation of all feature-gated binaries passed. No CLI, RPC, signing or broadcast was executed.

```console
cargo check --lib --locked
cargo test --lib --locked
cargo check --all-features --bins --locked
```

## License

Project-authored source code is licensed under the [MIT License](LICENSE). Third-party components and datasets retain their respective rights and licenses.

## Provider configuration

Optional provider credentials are read from the environment variables listed in `.env.example`. Providers with missing or blank values are omitted. Never commit a populated environment file. No credentials are bundled. Historical credentials must be rotated at their providers; deleting them from Git does not invalidate them.
