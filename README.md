# PSTU Payment Gateway & Money Movement Engine

High-performance, concurrency-safe payment gateway backend built with **Rust + Axum**, PostgreSQL 16 (source of truth), Redis 7 (caching/rate-limiting/session store), and NATS JetStream (lightweight event streaming).

---

## 🏛️ Architecture & Rationale

We selected **Rust + Axum** over alternatives like Python/FastAPI for mission-critical financial infrastructure:

- **Zero-Cost Concurrency & Safety**: Rust's compile-time borrow checker eliminates data races, pointer corruption, and memory hazards without runtime garbage collection pauses.
- **Ultra-High Throughput & Low Latency**: Powered by Tokio and Tower, Axum handles massive concurrent transaction volumes with minimal memory and CPU overhead.
- **Lock-Free / Zero-Contention Channels**: Inter-worker communication uses bounded `mpsc`/`crossbeam` channels (Actor model) instead of heavy `Arc<Mutex<T>>` locks to eliminate thread stalls.
- **Single Binary & Kubernetes Scalability**: Compiles into a standalone static binary with zero runtime dependencies, enabling tiny container images and instant horizontal scaling on Kubernetes.
- **Double-Entry Append-Only Ledger**: Balances are derived cache state; immutable `ledger` records (+1/-1) form the single authoritative source of truth.
- **Deadlock-Free Ascending Lock Ordering**: Invariant R5 enforced via sorted `user_id` locks before balance updates.
- **Transaction PIN Authentication & Defense**: 4-6 digit Argon2id PIN verification with 5-attempt/15min Redis lockout on all money-committing operations.
- **Human-Readable Crockford Base32 References**: Base32 `TRX...` and `FND...` tracking codes.
- **Dynamic CORS & CSRF Defense**: Flexible environment variable `CORS_ALLOWED_ORIGINS` with support for credentials, custom headers, and wildcard origins.

---

## 📦 Services Topology (`docker compose up -d`)

| Service | Image | Port | Purpose |
|---|---|---|---|
| **PostgreSQL** | `postgres:16-alpine` | `5432` | ACID data store, fillfactor=70 tables, BRIN & Trigram indexes |
| **Redis** | `redis:7-alpine` | `6379` | Distributed session cache, brute-force lockout, rate limiting |
| **NATS JetStream** | `nats:latest` | `4222`, `8222` | High-throughput durable event streaming & pub/sub |
| **Jaeger** | `jaegertracing/all-in-one:latest` | `16686`, `4317` | OpenTelemetry distributed tracing UI & OTLP collector |
| **Prometheus** | `prom/prometheus:latest` | `9090` | Time-series metrics scraper for `/metrics` |
| **Grafana** | `grafana/grafana:latest` | `3000` | Real-time dashboards (TPS, p95/p99 latency, error rates) |
| **API** | Multi-stage Rust | `8080` | Axum payment gateway backend |

---

## 🚀 Implementation Progress (`plan.md`)

### Phase 0: Infrastructure & Core Primitives
- [x] **T01**: Docker compose stack with Postgres 16, Redis 7, NATS, Jaeger, Prometheus, and Grafana.
- [x] **T02**: Database migration `0001_init.sql` with double-entry ledger, fillfactor=70 balances, `tx_history`, `notifications`, BRIN/Trigram indexes, and DB-level immutability triggers.
- [x] **T03**: Structured JSON logging and OpenTelemetry tracing setup (`x-request-id` propagation).
- [x] **T04**: Standard error envelope (`{"error":{"code","message","request_id","fields"?}}`).
- [x] **T05**: Kubernetes health probes (`/health/live`, `/health/ready`), graceful shutdown, and Prometheus metrics (`/metrics`).
- [x] **T10**: Integer `Paisa(i64)` money model, decimal string formatting, parse regex, and unit tests (C02, C13, C18).

### Phase 1: Authentication, PIN & User Management
- [x] **T06**: `POST /auth/register` (P1) with Argon2id password and PIN hashing, 10-digit account generation, 10M paisa initial funding seed, and `registered` audit event.
- [x] **T07**: `POST /auth/login` (P2) & `POST /auth/logout` (P3) with Redis 24h sessions, 5-attempt/15min brute-force lockout defense (C40, R15), and httpOnly cookies.
- [x] **T08**: `GET /me` (P4) user profile and `GET /users/lookup` (P12) with pg_trgm fuzzy search.
- [x] **T51**: `POST /me/pin/change` (P26) with current PIN verification and audit logging.
- [x] **T52**: `POST /me/pin/reset` (P27) with password verification and Redis secondary session invalidation.
- [x] **T54**: `GET /public/users/{account_number}` (P28) for QR payment scanning (no phone disclosure).

### Phase 2: Transfers & Money Movement Engine
- [x] **T11**: `POST /transfers` (P6, Workflow W2) with ascending `user_id` `SELECT ... FOR UPDATE` deadlock-free locking, atomic double-entry ledger rows, and versioned balance updates.
- [x] **T12**: Recipient phone/account number/UUID resolution.
- [x] **T14**: Asynchronous transaction event notification pipeline via NATS JetStream event publishing.
- [x] **T53**: Crockford Base32 `reference` code generation (`TRX...` / `FND...`) on all movements.

### Phase 3: Transaction History & Keyset Pagination
- [x] **T15**: `GET /me/transactions` (P5) with keyset cursor pagination (`idx_tx_history_user_id_id_desc`), status filters (`completed`, `rejected`, `flagged`), direction, and date range queries.
- [x] **T42**: `GET /me/activity` (P22) unified process events activity audit feed.
- [x] **T43**: `GET /me/statement.csv` (P25) statement CSV export with 10k safety gate (C45).

### Phase 4: Money Requests
- [x] **T21**: `POST /requests` (P8), `GET /requests` (P9), `POST /requests/{id}/accept` (P10), `POST /requests/{id}/reject` (P11), and `POST /requests/{id}/cancel` (P29) under Workflow W3.
- [x] **T23**: Direct acceptance and rejection tests (C11, C12, C64).

### Phase 5: Payment Links & AI Intent Parser
- [x] **T24-T26**: `POST /links` (P13), `GET /links/{token}` (P14), `POST /links/{token}/claim` (P15), `POST /links/{token}/cancel` (P16), and `GET /me/links` (P30) under Workflow W4/W5.
- [x] **T27-T28**: `POST /ai/parse` (P17) multi-model AI natural language intent grammar with human-in-the-loop validation (C29-C33).

### Phase 6: In-App Notifications
- [x] **T59**: `GET /me/notifications` (P31) with unread filtering, unread counter, and cursor pagination.
- [x] **T60**: `POST /me/notifications/read` (P32) marking single, batch, or all notifications as read.

### Phase 7: Reconciliation & Automated Verification
- [x] **T30**: Automated 3-contract double-entry ledger reconciliation CLI tool (`cargo run --bin reconcile` / `make reconcile` / `scripts/reconcile.sh`).

### Phase 8: Distributed Rate Limiting & Dynamic CORS
- [x] **T31**: Distributed sliding-window rate limiters across all API endpoints (transfers, links, AI parse, auth).
- [x] **T61**: Dynamic CORS configuration supporting environment variable `CORS_ALLOWED_ORIGINS` (or defaults to allow all).

---

## 🛠️ Developer Commands

```bash
# 1. Spin up only dependencies (Postgres, Redis, NATS, Jaeger) without building containers
make docker-dev

# 2. Run API backend locally on host
make api

# 3. Run frontend (if needed)
make web

# Stop containers without erasing volumes
make docker-down

# Erase containers, networks, and volumes (preserves base images)
make docker-erase

# Run automated end-to-end workflow demo
make demo

# Run 3-contract double-entry ledger reconciliation
make reconcile

# Run all quality checks
make check
make fmt-check
make clippy
make test
```
