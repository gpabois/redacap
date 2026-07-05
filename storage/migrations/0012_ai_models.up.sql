CREATE TABLE ai_models (
    id BYTEA PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    api_key_encrypted BYTEA NOT NULL,
    system_prompt TEXT NOT NULL DEFAULT '',
    active BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Au plus un modèle actif à la fois : le moteur de l'agent IA « Marie ».
CREATE UNIQUE INDEX ai_models_single_active_idx ON ai_models ((true)) WHERE active;
