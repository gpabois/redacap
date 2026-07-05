CREATE TABLE legal_acts (
    id BYTEA PRIMARY KEY,
    title TEXT NOT NULL,
    issuer_id BYTEA NOT NULL REFERENCES issuers(group_id),
    authority_id BYTEA NOT NULL REFERENCES authorities(id),
    status TEXT NOT NULL DEFAULT 'redaction',
    created_by BYTEA NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX legal_acts_authority_id_idx ON legal_acts (authority_id);
CREATE INDEX legal_acts_issuer_id_idx ON legal_acts (issuer_id);
CREATE INDEX legal_acts_created_by_idx ON legal_acts (created_by);
