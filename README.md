# PSTU Payment Gateway API

High-performance, concurrency-safe payment gateway backend built with Rust and Axum.

## Architecture & Framework Choice

We selected **Rust + Axum** over alternatives like Python/FastAPI for critical payment infrastructure:

- **Concurrency Safety**: Rust's ownership model and type system eliminate data races and concurrency hazards at compile time.
- **High Throughput & Low Latency**: Powered by Tokio and Tower, Axum provides high request throughput and minimal memory footprint under heavy financial transaction loads.
- **Data Race Reduction**: Financial state transitions, ledgers, and balances require strict thread safety without runtime interpreter locks.
- **Single Binary Artifact**: Compiles into a single standalone binary with no runtime dependencies, resulting in minimal Docker image footprints and rapid Kubernetes horizontal pod autoscaling.

## Project Structure

This service follows a modular, feature-based architecture:
- `src/core/`: Shared primitives (configuration, database pooling, global error handling, middleware).
- `src/features/`: Isolated vertical feature slices (auth, payments, refunds, webhooks, ledgers).

## Quality & Development Guidelines

All code changes and bug fixes must pass quality gates before commits:
```bash
cargo fmt --check
cargo check
cargo clippy -- -D warnings
cargo test
```