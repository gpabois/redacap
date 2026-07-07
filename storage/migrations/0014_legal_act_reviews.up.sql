CREATE TABLE legal_act_review_updates (
    legal_act_id BYTEA NOT NULL,
    seq BIGINT NOT NULL,
    update BYTEA NOT NULL,
    author_id BYTEA NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (legal_act_id, seq)
);

CREATE TABLE legal_act_review_snapshots (
    legal_act_id BYTEA PRIMARY KEY,
    snapshot BYTEA NOT NULL,
    seq BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
