CREATE TABLE agent_run_documents (
    id BYTEA PRIMARY KEY,
    run_id BYTEA NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    bytes BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DROP TABLE legal_act_documents;
