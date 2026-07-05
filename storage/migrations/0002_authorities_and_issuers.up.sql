CREATE TABLE authorities (
    id BYTEA PRIMARY KEY,
    nom TEXT NOT NULL,
    code TEXT NOT NULL UNIQUE,
    logo_url TEXT,
    tutelle TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE issuers (
    group_id BYTEA PRIMARY KEY REFERENCES groups(id) ON DELETE CASCADE,
    authority_id BYTEA NOT NULL REFERENCES authorities(id),
    libelle TEXT NOT NULL,
    formule_politesse TEXT,
    ordre_affichage INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
