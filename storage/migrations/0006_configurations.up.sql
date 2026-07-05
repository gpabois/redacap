CREATE TABLE configurations (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by BYTEA REFERENCES users(id) ON DELETE SET NULL
);
