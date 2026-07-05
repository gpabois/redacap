-- Domaines et intentions (voir Claude.md § Modalités agentiques / Structure) :
-- remplacent la notion d'"issuer" (signataire), retirée ci-dessous, qui ne
-- correspondait à aucune exigence.

CREATE TABLE domains (
    id BYTEA PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    agent_context TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE intentions (
    id BYTEA PRIMARY KEY,
    domain_id BYTEA NOT NULL REFERENCES domains(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    agent_context TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (domain_id, name)
);
CREATE INDEX intentions_domain_id_idx ON intentions (domain_id);

CREATE TABLE legal_act_intentions (
    legal_act_id BYTEA NOT NULL REFERENCES legal_acts(id) ON DELETE CASCADE,
    intention_id BYTEA NOT NULL REFERENCES intentions(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (legal_act_id, intention_id)
);

-- `domain_id` NULL = disponibilité globale (ex. Légifrance) ; sinon la ligne
-- porte la disponibilité de l'outil pour un domaine précis (ex. GéoRisques
-- réservé au domaine "Installation classée").
CREATE TABLE agent_tool_scopes (
    tool_name TEXT NOT NULL,
    domain_id BYTEA REFERENCES domains(id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX agent_tool_scopes_global_idx ON agent_tool_scopes (tool_name) WHERE domain_id IS NULL;
CREATE UNIQUE INDEX agent_tool_scopes_domain_idx ON agent_tool_scopes (tool_name, domain_id) WHERE domain_id IS NOT NULL;

-- Retrait de la notion d'"issuer" (signataire) au profit du domaine, fixé
-- une fois pour toutes à la création du projet.
ALTER TABLE legal_acts ADD COLUMN domain_id BYTEA NOT NULL REFERENCES domains(id);
CREATE INDEX legal_acts_domain_id_idx ON legal_acts (domain_id);
DROP INDEX legal_acts_issuer_id_idx;
ALTER TABLE legal_acts DROP COLUMN issuer_id;
DROP TABLE issuers;
