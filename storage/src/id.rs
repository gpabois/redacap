use shared::id::ID;
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::error::StorageError;

/// Convertit un identifiant applicatif vers sa représentation `BYTEA`.
pub(crate) fn encode(id: &ID) -> &[u8] {
    id.as_bytes()
}

/// Reconstruit un identifiant applicatif depuis sa représentation `BYTEA`.
pub(crate) fn decode(bytes: &[u8]) -> Result<ID, StorageError> {
    ID::try_from(bytes).map_err(|err| StorageError::InvalidId(err.to_string()))
}

/// Lit une colonne `BYTEA` non nullable et la convertit en identifiant.
pub(crate) fn column(row: &PgRow, name: &str) -> Result<ID, StorageError> {
    let bytes: Vec<u8> = row.try_get(name)?;
    decode(&bytes)
}

/// Lit une colonne `BYTEA` nullable et la convertit en identifiant optionnel.
pub(crate) fn column_opt(row: &PgRow, name: &str) -> Result<Option<ID>, StorageError> {
    let bytes: Option<Vec<u8>> = row.try_get(name)?;
    bytes.as_deref().map(decode).transpose()
}
