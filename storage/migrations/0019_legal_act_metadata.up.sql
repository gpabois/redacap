-- Métadonnées contextuelles d'un projet d'acte légal (installation,
-- rubriques ICPE, émissaires...), en paires clé/valeur JSON libre :
-- alimentées par l'inspecteur (panneau « Métadonnées » de l'éditeur) et par
-- l'agent IA (outils read_metadata/write_metadata/search_metadata).

CREATE TABLE legal_act_metadata (
    legal_act_id BYTEA NOT NULL REFERENCES legal_acts(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (legal_act_id, key)
);
