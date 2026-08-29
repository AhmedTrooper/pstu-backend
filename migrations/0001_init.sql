-- Enable Extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- 1. Users Table
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_number VARCHAR(20) UNIQUE NOT NULL,
    name TEXT NOT NULL CHECK (char_length(name) >= 1 AND char_length(name) <= 100),
    phone VARCHAR(20) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended', 'locked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Users Trigram Search Index (§14)
CREATE INDEX IF NOT EXISTS idx_users_name_trgm ON users USING gin (name gin_trgm_ops);

-- 2. Balances Table (fillfactor=70 for HOT updates, §14)
CREATE TABLE IF NOT EXISTS balances (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE RESTRICT,
    amount_paisa BIGINT NOT NULL CHECK (amount_paisa >= 0),
    version BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
) WITH (fillfactor = 70);

-- 3. Transfers Table (§2, §14)
CREATE TABLE IF NOT EXISTS transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    recipient_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    amount_paisa BIGINT NOT NULL CHECK (amount_paisa > 0),
    note TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 200),
    status TEXT NOT NULL DEFAULT 'completed' CHECK (status IN ('completed', 'rejected', 'flagged')),
    idempotency_key UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_transfers_distinct_participants CHECK (sender_id <> recipient_id)
);

-- 4. Ledger Table (Append-only, Double-entry, §2, §14)
CREATE TABLE IF NOT EXISTS ledger (
    id BIGSERIAL PRIMARY KEY,
    txn_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    counterparty_id UUID REFERENCES users(id) ON DELETE RESTRICT,
    direction SMALLINT NOT NULL CHECK (direction IN (-1, 1)),
    amount_paisa BIGINT NOT NULL CHECK (amount_paisa > 0),
    running_balance BIGINT NOT NULL CHECK (running_balance >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('funding', 'transfer_sent', 'transfer_received', 'request_paid', 'link_paid')),
    ref_id UUID,
    idempotency_key UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Ledger Indexes (§14)
CREATE INDEX IF NOT EXISTS idx_ledger_user_id_id_desc ON ledger (user_id, id DESC);
CREATE UNIQUE INDEX IF NOT EXISTS uq_ledger_txn_direction ON ledger (txn_id, direction);
CREATE UNIQUE INDEX IF NOT EXISTS uq_ledger_idempotency_key ON ledger (idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ledger_created_at_brin ON ledger USING brin (created_at);

-- 5. Money Requests Table (§2, §14)
CREATE TABLE IF NOT EXISTS money_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    requester_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    debtor_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    amount_paisa BIGINT NOT NULL CHECK (amount_paisa > 0),
    note TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 200),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    CONSTRAINT chk_requests_distinct_participants CHECK (requester_id <> debtor_id)
);

-- Money Requests Indexes (§14)
CREATE INDEX IF NOT EXISTS idx_requests_debtor ON money_requests (debtor_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_requester ON money_requests (requester_id, status, created_at DESC);

-- 6. Payment Links Table (§2, §14)
CREATE TABLE IF NOT EXISTS payment_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    amount_paisa BIGINT NOT NULL CHECK (amount_paisa > 0),
    note TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 120),
    token TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'claimed', 'expired', 'cancelled')),
    claimer_id UUID REFERENCES users(id) ON DELETE RESTRICT,
    transfer_id UUID REFERENCES transfers(id) ON DELETE RESTRICT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ
);

-- Payment Links Indexes (§14)
CREATE INDEX IF NOT EXISTS idx_links_creator ON payment_links (creator_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_links_active_expires ON payment_links (expires_at) WHERE status = 'active';

-- 7. Process Events Table (Immutable Audit Log, §2, §14)
CREATE TABLE IF NOT EXISTS process_events (
    id BIGSERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('transfer', 'request', 'link', 'auth', 'system')),
    entity_id UUID NOT NULL,
    event TEXT NOT NULL CHECK (event IN ('registered', 'login_success', 'login_failed', 'logout', 'created', 'completed', 'rejected', 'flagged', 'accepted', 'cancelled', 'expired', 'claimed')),
    actor_id UUID REFERENCES users(id) ON DELETE RESTRICT,
    reason TEXT NOT NULL DEFAULT '',
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Process Events Indexes (§14)
CREATE INDEX IF NOT EXISTS idx_events_entity ON process_events (entity_type, entity_id, id DESC);
CREATE INDEX IF NOT EXISTS idx_events_actor ON process_events (actor_id, id DESC) WHERE actor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_created_at_brin ON process_events USING brin (created_at);

-- 8. Immutability Enforcers (§2 C39)
CREATE OR REPLACE FUNCTION deny_ledger_and_event_mutations()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'MUTATION_DENIED: ledger and process_events rows are strictly immutable.';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_immutable_ledger ON ledger;
CREATE TRIGGER trg_immutable_ledger
BEFORE UPDATE OR DELETE ON ledger
FOR EACH ROW EXECUTE FUNCTION deny_ledger_and_event_mutations();

DROP TRIGGER IF EXISTS trg_immutable_process_events ON process_events;
CREATE TRIGGER trg_immutable_process_events
BEFORE UPDATE OR DELETE ON process_events
FOR EACH ROW EXECUTE FUNCTION deny_ledger_and_event_mutations();
