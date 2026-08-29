.PHONY: all docker docker-down docker-erase api test check fmt fmt-check clippy

# Default target
all: check test

# Spin up docker compose services (Postgres, Redis, API)
docker:
	docker compose up -d

# Spin down docker compose services
docker-down:
	docker compose down

# Stop and remove containers without deleting images or persistent volumes
docker-erase:
	docker compose down --remove-orphans

# Run the API locally
api:
	cargo run

# Run all test suites
test:
	cargo test --all-features

# Run fast type and dependency checks
check:
	cargo check

# Format Rust source code
fmt:
	cargo fmt

# Check Rust formatting without modifying files
fmt-check:
	cargo fmt --check

# Run Clippy with strict warning denials
clippy:
	cargo clippy -- -D warnings
