.PHONY: all build check fmt fmt-check clippy test api docker docker-dev docker-down docker-erase down-port reconcile demo

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
	cargo run --bin api

reconcile:
	cargo run --bin reconcile

demo:
	./scripts/demo.sh

docker-dev:
	docker compose up -d postgres redis nats jaeger

docker:
	docker compose up -d --build

docker-down:
	docker compose down

docker-erase:
	docker compose down -v --remove-orphans

down-port:
	./scripts/down_port.sh
