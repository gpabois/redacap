-- Session de conversation avec l'agent IA pour une salle (projet) : regroupe
-- une chaîne de agent_runs (voir agent_runs.session_id ci-dessous) démarrée
-- soit à l'ouverture de la salle, soit après l'archivage de la session
-- précédente (voir ClientMessage::ClearHistory côté server::editor::ws).
-- Permet à un inspecteur de consulter plus tard une conversation passée
-- sans jamais affecter la conversation en cours.
CREATE TABLE agent_sessions (
    id BYTEA PRIMARY KEY,
    room_id TEXT NOT NULL,
    started_by BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at TIMESTAMPTZ
);

-- Au plus une session active par salle : une nouvelle conversation ne peut
-- démarrer que si la précédente a été archivée.
CREATE UNIQUE INDEX agent_sessions_active_per_room_idx ON agent_sessions (room_id)
    WHERE status = 'active';
CREATE INDEX agent_sessions_room_id_idx ON agent_sessions (room_id);
CREATE INDEX agent_sessions_started_by_idx ON agent_sessions (started_by);

ALTER TABLE agent_runs ADD COLUMN session_id BYTEA REFERENCES agent_sessions(id) ON DELETE CASCADE;

-- Rétro-remplissage : une session active par salle déjà porteuse de runs,
-- démarrée par l'auteur de son run le plus ancien, à laquelle tous ses runs
-- sont rattachés — équivalent à l'ancien modèle « une seule conversation
-- continue par salle ». `uuid_send(gen_random_uuid())` fournit 16 octets
-- aléatoires (même format qu'un identifiant applicatif, voir shared::id::ID)
-- sans dépendre de l'extension pgcrypto (gen_random_uuid est native depuis
-- PostgreSQL 13).
INSERT INTO agent_sessions (id, room_id, started_by, status, created_at)
SELECT uuid_send(gen_random_uuid()), room_id, (array_agg(author_id ORDER BY created_at))[1], 'active', min(created_at)
FROM agent_runs
GROUP BY room_id;

UPDATE agent_runs r
SET session_id = s.id
FROM agent_sessions s
WHERE r.room_id = s.room_id;

ALTER TABLE agent_runs ALTER COLUMN session_id SET NOT NULL;
CREATE INDEX agent_runs_session_id_idx ON agent_runs (session_id);
