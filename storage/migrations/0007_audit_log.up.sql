CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor_id BYTEA REFERENCES users(id) ON DELETE SET NULL,
    actor_ip TEXT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id BYTEA,
    details JSONB
);
