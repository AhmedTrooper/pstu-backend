# Multi-stage build for minimal production image
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency files for layer caching
COPY Cargo.toml Cargo.lock ./

# Copy source code and build release binary
COPY src ./src
COPY migrations ./migrations

RUN cargo build --release --bin api

# Production minimal runtime
FROM debian:bookworm-slim AS runtime

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -u 10001 -m -s /bin/bash appuser

# Copy binary from builder
COPY --from=builder /app/target/release/api /usr/local/bin/api

USER appuser

EXPOSE 8080

ENV HOST=0.0.0.0 \
    PORT=8080 \
    RUST_LOG=info,api=debug

CMD ["/usr/local/bin/api"]
