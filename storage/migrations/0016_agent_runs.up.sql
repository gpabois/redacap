-- État persisté d'une orchestration hiérarchique (voir
-- agent::orchestration::OrchestrationRun) : ce qui permet à une pause HITL
-- (question posée à l'inspecteur, confirmation requise...) de survivre à une
-- déconnexion ou un redémarrage du serveur.
CREATE TABLE agent_runs (
    id BYTEA PRIMARY KEY,
    room_id TEXT NOT NULL,
    author_id BYTEA NOT NULL,
    status TEXT NOT NULL,
    stack JSONB NOT NULL,
    final_answer TEXT,
    version INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Au plus un run actif (en cours ou en pause) par salle : une nouvelle tâche
-- ne peut être lancée tant que la précédente n'est pas terminée.
CREATE UNIQUE INDEX agent_runs_active_per_room_idx ON agent_runs (room_id)
    WHERE status IN ('running', 'paused');
CREATE INDEX agent_runs_room_id_idx ON agent_runs (room_id);

-- Documents uploadés par l'inspecteur en réponse à une pause de type
-- `request_document`, persistés indépendamment de la connexion websocket qui
-- a formulé la demande (voir server::editor::ports::WsUserInteraction, qui
-- les conservait auparavant en mémoire pour la seule durée de la connexion).
CREATE TABLE agent_run_documents (
    id BYTEA PRIMARY KEY,
    run_id BYTEA NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    bytes BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
