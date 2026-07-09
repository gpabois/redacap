-- Documents fournis par l'inspecteur, désormais rattachés au projet d'acte
-- légal lui-même plutôt qu'à une session de conversation avec l'agent (voir
-- agent_run_documents, migration 0016) : ils restent ainsi disponibles d'une
-- session à l'autre (une session archivée n'en coupait plus l'accès qu'à
-- travers son propre historique) et sont administrables directement depuis
-- un panneau dédié de l'éditeur, indépendamment de toute conversation avec
-- l'agent.
CREATE TABLE legal_act_documents (
    id BYTEA PRIMARY KEY,
    legal_act_id BYTEA NOT NULL REFERENCES legal_acts(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    bytes BYTEA NOT NULL,
    uploaded_by BYTEA NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX legal_act_documents_legal_act_id_idx ON legal_act_documents (legal_act_id);

-- Rétro-remplissage : documents déjà uploadés via `request_document`,
-- rattachés au projet de la salle qui les a produits (`room_id` est
-- l'identifiant du projet encodé en hexadécimal, voir shared::id::ID) ;
-- `uploaded_by` retombe sur l'auteur du run, faute de mieux (l'ancien modèle
-- ne distinguait pas qui avait effectivement fourni le fichier). Les
-- documents dont la salle ne correspond à aucun projet existant (salle de
-- test, projet supprimé depuis) ne sont pas repris.
INSERT INTO legal_act_documents (id, legal_act_id, file_name, mime_type, bytes, uploaded_by, created_at)
SELECT d.id, la.id, d.file_name, d.mime_type, d.bytes, r.author_id, d.created_at
FROM agent_run_documents d
JOIN agent_runs r ON r.id = d.run_id
JOIN legal_acts la ON la.id = decode(r.room_id, 'hex')
WHERE r.room_id ~ '^[0-9a-f]{32}$';

DROP TABLE agent_run_documents;
