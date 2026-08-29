.PHONY: all build check fmt fmt-check clippy test api docker docker-down docker-erase reconcile

all: check test

build:
	cargo build

check:
	cargo check

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

clippy:
	cargo clippy -- -D warnings

test:
	cargo test

api:
	cargo run

reconcile:
	cargo run --bin reconcile

docker:
	docker compose up -d --build

docker-down:
	docker compose down

docker-erase:
	docker compose down -v --remove-orphans
