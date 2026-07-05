use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::model::{Configuration, ConfigurationChangeset, CreateConfiguration};

fn from_row(row: PgRow) -> Result<Configuration, StorageError> {
    Ok(Configuration {
        key: row.try_get("key")?,
        value: row.try_get("value")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: id::column_opt(&row, "updated_by")?,
    })
}

/// Crée un nouveau paramètre de configuration.
///
/// Échoue si la clé existe déjà ; utiliser [`update_configuration`] pour la faire évoluer.
pub async fn create_configuration(
    pool: &Pool,
    args: CreateConfiguration,
) -> Result<Configuration, StorageError> {
    let row = sqlx::query(
        "INSERT INTO configurations (key, value, updated_by) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(args.key)
    .bind(args.value)
    .bind(args.updated_by.as_ref().map(id::encode))
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère un paramètre de configuration par sa clé.
pub async fn get_configuration(pool: &Pool, key: &str) -> Result<Configuration, StorageError> {
    let row = sqlx::query("SELECT * FROM configurations WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste l'ensemble des paramètres de configuration.
pub async fn list_configurations(pool: &Pool) -> Result<Vec<Configuration>, StorageError> {
    let rows = sqlx::query("SELECT * FROM configurations ORDER BY key")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie d'un paramètre de configuration existant.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle. `updated_by: Some(None)` l'efface.
pub async fn update_configuration(
    pool: &Pool,
    key: &str,
    changeset: ConfigurationChangeset,
) -> Result<Configuration, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE configurations SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(value) = changeset.value {
        set.push("value = ").push_bind_unseparated(value);
    }
    if let Some(updated_by) = changeset.updated_by {
        set.push("updated_by = ")
            .push_bind_unseparated(updated_by.map(|id| id.as_bytes().to_vec()));
    }
    builder.push(" WHERE key = ").push_bind(key.to_string());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime un paramètre de configuration.
pub async fn delete_configuration(pool: &Pool, key: &str) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM configurations WHERE key = $1")
        .bind(key)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
