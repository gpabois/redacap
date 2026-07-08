//! Persistance de l'état d'une orchestration hiérarchique (voir migration
//! `0016_agent_runs`) : ce qui permet à une pause HITL de survivre à une
//! déconnexion ou un redémarrage du serveur (voir
//! `agent::orchestration::OrchestrationRun`). Ce module ignore la structure
//! interne de `stack` — c'est `server` qui la (dé)sérialise depuis/vers
//! `agent::orchestration::AgentFrame`.

use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{AgentRun, AgentRunDocument};

fn from_row(row: PgRow) -> Result<AgentRun, StorageError> {
    Ok(AgentRun {
        id: id::column(&row, "id")?,
        room_id: row.try_get("room_id")?,
        session_id: id::column(&row, "session_id")?,
        author_id: id::column(&row, "author_id")?,
        status: row.try_get("status")?,
        stack: row.try_get("stack")?,
        final_answer: row.try_get("final_answer")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn document_from_row(row: PgRow) -> Result<AgentRunDocument, StorageError> {
    Ok(AgentRunDocument {
        id: id::column(&row, "id")?,
        run_id: id::column(&row, "run_id")?,
        file_name: row.try_get("file_name")?,
        mime_type: row.try_get("mime_type")?,
        bytes: row.try_get("bytes")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Crée un nouveau run en cours (`status = "running"`) pour `room_id`,
/// rattaché à `session_id` (voir `storage::agent_session`). Échoue si un run
/// `running`/`paused` existe déjà pour cette salle (voir
/// `agent_runs_active_per_room_idx`).
pub async fn create_run(
    pool: &Pool,
    room_id: &str,
    session_id: &ID,
    author_id: &ID,
    stack: Value,
) -> Result<AgentRun, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO agent_runs (id, room_id, session_id, author_id, status, stack) \
         VALUES ($1, $2, $3, $4, 'running', $5) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(room_id)
    .bind(id::encode(session_id))
    .bind(id::encode(author_id))
    .bind(stack)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère le run actif (`running` ou `paused`) de `room_id`, s'il en
/// existe un : au plus un à la fois (voir `agent_runs_active_per_room_idx`).
pub async fn get_active_run_for_room(
    pool: &Pool,
    room_id: &str,
) -> Result<Option<AgentRun>, StorageError> {
    let row = sqlx::query(
        "SELECT * FROM agent_runs WHERE room_id = $1 AND status IN ('running', 'paused')",
    )
    .bind(room_id)
    .fetch_optional(pool)
    .await?;
    row.map(from_row).transpose()
}

/// Récupère le run le plus récent de `session_id`, quel que soit son statut :
/// utilisé pour reprendre la conversation d'un run `"done"` sur une tâche
/// suivante (voir `agent::orchestration::AgentFrame::resume_as_new_task`), et
/// pour reconstruire le transcript affiché lors de la consultation d'une
/// session passée (voir `ClientMessage::GetAgentSessionHistory`) — son
/// historique cumulatif (`stack[0].history`) couvre toute la session tant que
/// chaque run a bien été enchaîné sur le précédent (voir
/// `server::editor::ws::run_orchestration`).
pub async fn get_latest_run_for_session(
    pool: &Pool,
    session_id: &ID,
) -> Result<Option<AgentRun>, StorageError> {
    let row = sqlx::query(
        "SELECT * FROM agent_runs WHERE session_id = $1 ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(id::encode(session_id))
    .fetch_optional(pool)
    .await?;
    row.map(from_row).transpose()
}

/// Récupère le tout premier run de `session_id` (voir
/// [`get_latest_run_for_session`]) : sa première tâche utilisateur sert
/// d'aperçu de la session dans la liste (voir
/// `ClientMessage::ListAgentSessions`).
pub async fn get_earliest_run_for_session(
    pool: &Pool,
    session_id: &ID,
) -> Result<Option<AgentRun>, StorageError> {
    let row = sqlx::query(
        "SELECT * FROM agent_runs WHERE session_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(id::encode(session_id))
    .fetch_optional(pool)
    .await?;
    row.map(from_row).transpose()
}

/// Bascule à `"failed"` tout run resté `"running"` (voir
/// `agent_runs_active_per_room_idx`) : à appeler une fois au démarrage du
/// serveur (voir `server::run`), avant qu'aucune connexion ne soit acceptée.
/// Un run `"running"` ne peut survivre à un redémarrage du processus qui le
/// pilotait — aucune tâche Tokio en mémoire ne va jamais le faire progresser
/// ni le persister à `"done"`/`"failed"` — sans cette purge, il resterait
/// indéfiniment actif et bloquerait `agent_runs_active_per_room_idx`,
/// empêchant tout nouveau [`create_run`] sur sa salle. Un run `"paused"`
/// n'est jamais concerné : il attend légitimement une réponse de
/// l'inspecteur et reste repris normalement (voir
/// `server::editor::ws::replay_pending_interaction`).
pub async fn fail_orphaned_running_runs(pool: &Pool) -> Result<u64, StorageError> {
    let result = sqlx::query("UPDATE agent_runs SET status = 'failed', version = version + 1, \
         updated_at = now() WHERE status = 'running'")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Persiste l'avancement d'un run : n'a d'effet que si `version` correspond
/// encore à la valeur en base (verrou optimiste), pour détecter une reprise
/// concurrente plutôt que d'écraser silencieusement l'état d'une
/// orchestration déjà avancée par un autre worker. Échoue avec
/// [`StorageError::Conflict`] si `version` ne correspond plus.
pub async fn save_run(
    pool: &Pool,
    run_id: &ID,
    version: i32,
    status: &str,
    stack: Value,
    final_answer: Option<&str>,
) -> Result<AgentRun, StorageError> {
    let row = sqlx::query(
        "UPDATE agent_runs SET status = $1, stack = $2, final_answer = $3, \
         version = version + 1, updated_at = now() \
         WHERE id = $4 AND version = $5 RETURNING *",
    )
    .bind(status)
    .bind(stack)
    .bind(final_answer)
    .bind(id::encode(run_id))
    .bind(version)
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::Conflict)?;
    from_row(row)
}

/// Enregistre un document uploadé par l'inspecteur en réponse à une pause de
/// type `request_document`, rattaché à `run_id`.
pub async fn store_document(
    pool: &Pool,
    run_id: &ID,
    file_name: &str,
    mime_type: &str,
    bytes: Vec<u8>,
) -> Result<AgentRunDocument, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO agent_run_documents (id, run_id, file_name, mime_type, bytes) \
         VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(id::encode(run_id))
    .bind(file_name)
    .bind(mime_type)
    .bind(bytes)
    .fetch_one(pool)
    .await?;
    document_from_row(row)
}

/// Relit un document précédemment stocké via [`store_document`], pour
/// l'outil `read_document` de l'agent.
pub async fn fetch_document(
    pool: &Pool,
    document_id: &ID,
) -> Result<AgentRunDocument, StorageError> {
    let row = sqlx::query("SELECT * FROM agent_run_documents WHERE id = $1")
        .bind(id::encode(document_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    document_from_row(row)
}
