use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreateOidcProvider, OidcProvider, OidcProviderChangeset};

fn from_row(row: PgRow) -> Result<OidcProvider, StorageError> {
    Ok(OidcProvider {
        id: id::column(&row, "id")?,
        name: row.try_get("name")?,
        issuer_url: row.try_get("issuer_url")?,
        client_id: row.try_get("client_id")?,
        client_secret_encrypted: row.try_get("client_secret_encrypted")?,
        scopes: row.try_get("scopes")?,
        active: row.try_get("active")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Enregistre un fournisseur OpenID Connect.
pub async fn create_oidc_provider(
    pool: &Pool,
    args: CreateOidcProvider,
) -> Result<OidcProvider, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO oidc_providers (id, name, issuer_url, client_id, client_secret_encrypted, scopes) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.name)
    .bind(args.issuer_url)
    .bind(args.client_id)
    .bind(args.client_secret_encrypted)
    .bind(args.scopes)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère un fournisseur OpenID Connect par son identifiant.
pub async fn get_oidc_provider(
    pool: &Pool,
    provider_id: &ID,
) -> Result<OidcProvider, StorageError> {
    let row = sqlx::query("SELECT * FROM oidc_providers WHERE id = $1")
        .bind(id::encode(provider_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste les fournisseurs OpenID Connect actifs.
pub async fn list_active_oidc_providers(pool: &Pool) -> Result<Vec<OidcProvider>, StorageError> {
    let rows = sqlx::query("SELECT * FROM oidc_providers WHERE active ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie de la configuration d'un fournisseur OpenID Connect.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle.
pub async fn update_oidc_provider(
    pool: &Pool,
    provider_id: &ID,
    changeset: OidcProviderChangeset,
) -> Result<OidcProvider, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE oidc_providers SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(name) = changeset.name {
        set.push("name = ").push_bind_unseparated(name);
    }
    if let Some(issuer_url) = changeset.issuer_url {
        set.push("issuer_url = ").push_bind_unseparated(issuer_url);
    }
    if let Some(client_id) = changeset.client_id {
        set.push("client_id = ").push_bind_unseparated(client_id);
    }
    if let Some(client_secret_encrypted) = changeset.client_secret_encrypted {
        set.push("client_secret_encrypted = ")
            .push_bind_unseparated(client_secret_encrypted);
    }
    if let Some(scopes) = changeset.scopes {
        set.push("scopes = ").push_bind_unseparated(scopes);
    }
    if let Some(active) = changeset.active {
        set.push("active = ").push_bind_unseparated(active);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(provider_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime un fournisseur OpenID Connect.
pub async fn delete_oidc_provider(pool: &Pool, provider_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM oidc_providers WHERE id = $1")
        .bind(id::encode(provider_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
