//! Persistance des métadonnées contextuelles d'un projet d'acte légal (voir
//! migration `0019_legal_act_metadata`) : paires clé/valeur JSON libre,
//! alimentées par l'inspecteur (panneau « Métadonnées » de l'éditeur) comme
//! par l'agent IA (outils `read_metadata`/`write_metadata`/`search_metadata`).

use sqlx::Row;
use sqlx::postgres::PgRow;
use serde_json::Value;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::LegalActMetadata;

fn from_row(row: PgRow) -> Result<LegalActMetadata, StorageError> {
    Ok(LegalActMetadata {
        legal_act_id: id::column(&row, "legal_act_id")?,
        key: row.try_get("key")?,
        value: row.try_get("value")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée ou remplace la métadonnée `key` d'un projet d'acte légal.
pub async fn upsert_metadata(
    pool: &Pool,
    legal_act_id: &ID,
    key: &str,
    value: Value,
) -> Result<LegalActMetadata, StorageError> {
    let row = sqlx::query(
        "INSERT INTO legal_act_metadata (legal_act_id, key, value) VALUES ($1, $2, $3) \
         ON CONFLICT (legal_act_id, key) DO UPDATE \
         SET value = excluded.value, updated_at = now() \
         RETURNING *",
    )
    .bind(id::encode(legal_act_id))
    .bind(key)
    .bind(value)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère la métadonnée `key` d'un projet d'acte légal, si elle existe.
pub async fn get_metadata(
    pool: &Pool,
    legal_act_id: &ID,
    key: &str,
) -> Result<Option<LegalActMetadata>, StorageError> {
    let row = sqlx::query("SELECT * FROM legal_act_metadata WHERE legal_act_id = $1 AND key = $2")
        .bind(id::encode(legal_act_id))
        .bind(key)
        .fetch_optional(pool)
        .await?;
    row.map(from_row).transpose()
}

/// Liste l'ensemble des métadonnées d'un projet d'acte légal, triées par clé.
pub async fn list_metadata(
    pool: &Pool,
    legal_act_id: &ID,
) -> Result<Vec<LegalActMetadata>, StorageError> {
    let rows = sqlx::query("SELECT * FROM legal_act_metadata WHERE legal_act_id = $1 ORDER BY key")
        .bind(id::encode(legal_act_id))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Supprime la métadonnée `key` d'un projet d'acte légal.
pub async fn delete_metadata(
    pool: &Pool,
    legal_act_id: &ID,
    key: &str,
) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM legal_act_metadata WHERE legal_act_id = $1 AND key = $2")
        .bind(id::encode(legal_act_id))
        .bind(key)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
