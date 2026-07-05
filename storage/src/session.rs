use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreateSession, Session};

fn from_row(row: PgRow) -> Result<Session, StorageError> {
    Ok(Session {
        id: id::column(&row, "id")?,
        user_id: id::column(&row, "user_id")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

/// Crée une session pour un utilisateur authentifié.
pub async fn create_session(pool: &Pool, args: CreateSession) -> Result<Session, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(id::encode(&args.user_id))
    .bind(args.expires_at)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère une session non expirée par son identifiant.
///
/// Renvoie [`StorageError::NotFound`] aussi bien si la session n'existe pas que si elle
/// est expirée, afin de ne pas distinguer ces deux cas côté appelant.
pub async fn get_active_session(pool: &Pool, session_id: &ID) -> Result<Session, StorageError> {
    let row = sqlx::query("SELECT * FROM sessions WHERE id = $1 AND expires_at > now()")
        .bind(id::encode(session_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime une session (déconnexion explicite d'un cookie donné).
pub async fn delete_session(pool: &Pool, session_id: &ID) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(id::encode(session_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Supprime toutes les sessions d'un utilisateur.
///
/// Utilisé pour la propagation immédiate des révocations (suspension de compte,
/// suppression d'utilisateur) : cf. contrainte racine « Propagation des révocations ».
pub async fn delete_sessions_for_user(pool: &Pool, user_id: &ID) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id::encode(user_id))
        .execute(pool)
        .await?;
    Ok(())
}
