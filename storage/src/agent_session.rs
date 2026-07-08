//! Persistance des sessions de conversation avec l'agent IA (voir migration
//! `0017_agent_sessions`) : une session regroupe la chaîne de
//! [`shared::model::AgentRun`] d'une salle entre son démarrage et son
//! archivage (voir [`archive_active_session_for_room`]), pour permettre à un
//! inspecteur de consulter plus tard une conversation passée sans affecter
//! celle en cours.

use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::AgentSession;

fn from_row(row: PgRow) -> Result<AgentSession, StorageError> {
    Ok(AgentSession {
        id: id::column(&row, "id")?,
        room_id: row.try_get("room_id")?,
        started_by: id::column(&row, "started_by")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        archived_at: row.try_get("archived_at")?,
    })
}

/// Crée une nouvelle session active pour `room_id`. Échoue si une session
/// active existe déjà pour cette salle (voir `agent_sessions_active_per_room_idx`)
/// — à l'appelant de vérifier via [`get_active_session_for_room`] au préalable.
pub async fn create_session(
    pool: &Pool,
    room_id: &str,
    started_by: &ID,
) -> Result<AgentSession, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO agent_sessions (id, room_id, started_by, status) \
         VALUES ($1, $2, $3, 'active') RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(room_id)
    .bind(id::encode(started_by))
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère une session par son identifiant, quel que soit son statut.
pub async fn get_session(
    pool: &Pool,
    session_id: &ID,
) -> Result<Option<AgentSession>, StorageError> {
    let row = sqlx::query("SELECT * FROM agent_sessions WHERE id = $1")
        .bind(id::encode(session_id))
        .fetch_optional(pool)
        .await?;
    row.map(from_row).transpose()
}

/// Récupère la session active (`"active"`) de `room_id`, s'il en existe une :
/// au plus une à la fois (voir `agent_sessions_active_per_room_idx`).
pub async fn get_active_session_for_room(
    pool: &Pool,
    room_id: &str,
) -> Result<Option<AgentSession>, StorageError> {
    let row = sqlx::query("SELECT * FROM agent_sessions WHERE room_id = $1 AND status = 'active'")
        .bind(room_id)
        .fetch_optional(pool)
        .await?;
    row.map(from_row).transpose()
}

/// Archive la session active de `room_id` (voir `ClientMessage::ClearHistory`) :
/// la prochaine tâche démarrée sur cette salle ouvre une nouvelle session
/// plutôt que de poursuivre la conversation archivée, qui reste consultable
/// en lecture seule (voir [`list_sessions_for_room`]). Sans effet (renvoie
/// `None`) si aucune session n'est active — à l'appelant de vérifier au
/// préalable qu'aucun run n'est `running`/`paused` sur cette salle.
pub async fn archive_active_session_for_room(
    pool: &Pool,
    room_id: &str,
) -> Result<Option<AgentSession>, StorageError> {
    let row = sqlx::query(
        "UPDATE agent_sessions SET status = 'archived', archived_at = now() \
         WHERE room_id = $1 AND status = 'active' RETURNING *",
    )
    .bind(room_id)
    .fetch_optional(pool)
    .await?;
    row.map(from_row).transpose()
}

/// Liste les sessions (actives et archivées) de `room_id` démarrées par
/// `started_by`, les plus récentes en premier (voir
/// `ClientMessage::ListAgentSessions`) : une session n'est proposée qu'à
/// l'utilisateur qui l'a démarrée, pas à l'ensemble des collaborateurs de la
/// salle.
pub async fn list_sessions_for_room(
    pool: &Pool,
    room_id: &str,
    started_by: &ID,
) -> Result<Vec<AgentSession>, StorageError> {
    let rows = sqlx::query(
        "SELECT * FROM agent_sessions WHERE room_id = $1 AND started_by = $2 \
         ORDER BY created_at DESC",
    )
    .bind(room_id)
    .bind(id::encode(started_by))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(from_row).collect()
}
