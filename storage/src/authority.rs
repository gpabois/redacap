use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{Authority, AuthorityChangeset, CreateAuthority};

fn from_row(row: PgRow) -> Result<Authority, StorageError> {
    Ok(Authority {
        id: id::column(&row, "id")?,
        nom: row.try_get("nom")?,
        code: row.try_get("code")?,
        logo_url: row.try_get("logo_url")?,
        tutelle: row.try_get("tutelle")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée une autorité administrative.
pub async fn create_authority(
    pool: &Pool,
    args: CreateAuthority,
) -> Result<Authority, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO authorities (id, nom, code, logo_url, tutelle) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.nom)
    .bind(args.code)
    .bind(args.logo_url)
    .bind(args.tutelle)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère une autorité par son identifiant.
pub async fn get_authority(pool: &Pool, authority_id: &ID) -> Result<Authority, StorageError> {
    let row = sqlx::query("SELECT * FROM authorities WHERE id = $1")
        .bind(id::encode(authority_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste l'ensemble des autorités administratives.
pub async fn list_authorities(pool: &Pool) -> Result<Vec<Authority>, StorageError> {
    let rows = sqlx::query("SELECT * FROM authorities ORDER BY nom")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie des attributs d'une autorité administrative.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle. `logo_url`/`tutelle` à `Some(None)` les efface.
pub async fn update_authority(
    pool: &Pool,
    authority_id: &ID,
    changeset: AuthorityChangeset,
) -> Result<Authority, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE authorities SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(nom) = changeset.nom {
        set.push("nom = ").push_bind_unseparated(nom);
    }
    if let Some(code) = changeset.code {
        set.push("code = ").push_bind_unseparated(code);
    }
    if let Some(logo_url) = changeset.logo_url {
        set.push("logo_url = ").push_bind_unseparated(logo_url);
    }
    if let Some(tutelle) = changeset.tutelle {
        set.push("tutelle = ").push_bind_unseparated(tutelle);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(authority_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime une autorité administrative.
///
/// Échoue si un projet d'acte légal (`legal_acts.authority_id`) y est encore rattaché.
pub async fn delete_authority(pool: &Pool, authority_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM authorities WHERE id = $1")
        .bind(id::encode(authority_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
