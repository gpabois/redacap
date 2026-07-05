-- Configuration chiffrée des accès aux API externes (GéoRisques, Légifrance),
-- gérable depuis /admin/integrations. Table « singleton » par service (voir
-- contrainte `CHECK (id = 1)`) : au plus une ligne, mise à jour par upsert.

CREATE TABLE georisques_credentials (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    api_key_encrypted BYTEA,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by BYTEA REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE legifrance_credentials (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    client_id TEXT,
    client_secret_encrypted BYTEA,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by BYTEA REFERENCES users(id) ON DELETE SET NULL
);
