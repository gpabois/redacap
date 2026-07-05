use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreateDomain, Domain, DomainChangeset};

fn from_row(row: PgRow) -> Result<Domain, StorageError> {
    Ok(Domain {
        id: id::column(&row, "id")?,
        name: row.try_get("name")?,
        agent_context: row.try_get("agent_context")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée un domaine technique.
pub async fn create_domain(pool: &Pool, args: CreateDomain) -> Result<Domain, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO domains (id, name, agent_context) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.name)
    .bind(args.agent_context)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère un domaine par son identifiant.
pub async fn get_domain(pool: &Pool, domain_id: &ID) -> Result<Domain, StorageError> {
    let row = sqlx::query("SELECT * FROM domains WHERE id = $1")
        .bind(id::encode(domain_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste l'ensemble des domaines.
pub async fn list_domains(pool: &Pool) -> Result<Vec<Domain>, StorageError> {
    let rows = sqlx::query("SELECT * FROM domains ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie des attributs d'un domaine.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle.
pub async fn update_domain(
    pool: &Pool,
    domain_id: &ID,
    changeset: DomainChangeset,
) -> Result<Domain, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE domains SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(name) = changeset.name {
        set.push("name = ").push_bind_unseparated(name);
    }
    if let Some(agent_context) = changeset.agent_context {
        set.push("agent_context = ")
            .push_bind_unseparated(agent_context);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(domain_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime un domaine.
pub async fn delete_domain(pool: &Pool, domain_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM domains WHERE id = $1")
        .bind(id::encode(domain_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
