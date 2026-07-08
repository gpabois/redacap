-- Catalogue des agents experts éphémères que le Superviseur peut instancier
-- à la volée (voir agent::catalog::AgentCatalog) : chaque expert n'est
-- qu'une donnée éditable ici (voir /admin/agent-profiles), jamais une struct
-- Rust dédiée.
CREATE TABLE agent_profiles (
    id BYTEA PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    system_prompt TEXT NOT NULL DEFAULT '',
    tool_names TEXT[] NOT NULL DEFAULT '{}',
    max_steps INT NOT NULL DEFAULT 8,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
