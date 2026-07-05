-- Best-effort, à l'image de 0010_legal_act_projects.down.sql : ne
-- réaffecte pas les lignes existantes (perdues), recrée seulement le schéma.

CREATE TABLE issuers (
    group_id BYTEA PRIMARY KEY REFERENCES groups(id) ON DELETE CASCADE,
    authority_id BYTEA NOT NULL REFERENCES authorities(id),
    libelle TEXT NOT NULL,
    formule_politesse TEXT,
    ordre_affichage INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE legal_acts ADD COLUMN issuer_id BYTEA REFERENCES issuers(group_id);
CREATE INDEX legal_acts_issuer_id_idx ON legal_acts (issuer_id);

DROP INDEX legal_acts_domain_id_idx;
ALTER TABLE legal_acts DROP COLUMN domain_id;

DROP TABLE agent_tool_scopes;
DROP TABLE legal_act_intentions;
DROP TABLE intentions;
DROP TABLE domains;
