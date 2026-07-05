use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::model::{
    GeorisquesCredentials, LegifranceCredentials, SetGeorisquesCredentials,
    SetLegifranceCredentials,
};

fn georisques_from_row(row: PgRow) -> Result<GeorisquesCredentials, StorageError> {
    Ok(GeorisquesCredentials {
        api_key_encrypted: row.try_get("api_key_encrypted")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: id::column_opt(&row, "updated_by")?,
    })
}

/// Récupère la configuration GéoRisques, si elle a déjà été enregistrée.
pub async fn get_georisques_credentials(
    pool: &Pool,
) -> Result<Option<GeorisquesCredentials>, StorageError> {
    let row = sqlx::query("SELECT * FROM georisques_credentials WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    row.map(georisques_from_row).transpose()
}

/// Enregistre ou remplace la configuration GéoRisques (ligne singleton).
pub async fn set_georisques_credentials(
    pool: &Pool,
    args: SetGeorisquesCredentials,
) -> Result<GeorisquesCredentials, StorageError> {
    let row = sqlx::query(
        "INSERT INTO georisques_credentials (id, api_key_encrypted, updated_by) \
         VALUES (1, $1, $2) \
         ON CONFLICT (id) DO UPDATE SET \
             api_key_encrypted = EXCLUDED.api_key_encrypted, \
             updated_by = EXCLUDED.updated_by, \
             updated_at = now() \
         RETURNING *",
    )
    .bind(args.api_key_encrypted)
    .bind(args.updated_by.as_ref().map(id::encode))
    .fetch_one(pool)
    .await?;
    georisques_from_row(row)
}

fn legifrance_from_row(row: PgRow) -> Result<LegifranceCredentials, StorageError> {
    Ok(LegifranceCredentials {
        client_id: row.try_get("client_id")?,
        client_secret_encrypted: row.try_get("client_secret_encrypted")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: id::column_opt(&row, "updated_by")?,
    })
}

/// Récupère la configuration Légifrance, si elle a déjà été enregistrée.
pub async fn get_legifrance_credentials(
    pool: &Pool,
) -> Result<Option<LegifranceCredentials>, StorageError> {
    let row = sqlx::query("SELECT * FROM legifrance_credentials WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    row.map(legifrance_from_row).transpose()
}

/// Enregistre ou remplace la configuration Légifrance (ligne singleton).
pub async fn set_legifrance_credentials(
    pool: &Pool,
    args: SetLegifranceCredentials,
) -> Result<LegifranceCredentials, StorageError> {
    let row = sqlx::query(
        "INSERT INTO legifrance_credentials (id, client_id, client_secret_encrypted, updated_by) \
         VALUES (1, $1, $2, $3) \
         ON CONFLICT (id) DO UPDATE SET \
             client_id = EXCLUDED.client_id, \
             client_secret_encrypted = EXCLUDED.client_secret_encrypted, \
             updated_by = EXCLUDED.updated_by, \
             updated_at = now() \
         RETURNING *",
    )
    .bind(args.client_id)
    .bind(args.client_secret_encrypted)
    .bind(args.updated_by.as_ref().map(id::encode))
    .fetch_one(pool)
    .await?;
    legifrance_from_row(row)
}
