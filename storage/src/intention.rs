use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreateIntention, Intention, IntentionChangeset};

fn from_row(row: PgRow) -> Result<Intention, StorageError> {
    Ok(Intention {
        id: id::column(&row, "id")?,
        domain_id: id::column(&row, "domain_id")?,
        name: row.try_get("name")?,
        agent_context: row.try_get("agent_context")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée une intention rattachée à un domaine.
pub async fn create_intention(
    pool: &Pool,
    args: CreateIntention,
) -> Result<Intention, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO intentions (id, domain_id, name, agent_context) \
         VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(id::encode(&args.domain_id))
    .bind(args.name)
    .bind(args.agent_context)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère une intention par son identifiant.
pub async fn get_intention(pool: &Pool, intention_id: &ID) -> Result<Intention, StorageError> {
    let row = sqlx::query("SELECT * FROM intentions WHERE id = $1")
        .bind(id::encode(intention_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste l'ensemble des intentions, toutes domaines confondus.
pub async fn list_intentions(pool: &Pool) -> Result<Vec<Intention>, StorageError> {
    let rows = sqlx::query("SELECT * FROM intentions ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Liste les intentions rattachées à un domaine.
pub async fn list_intentions_by_domain(
    pool: &Pool,
    domain_id: &ID,
) -> Result<Vec<Intention>, StorageError> {
    let rows = sqlx::query("SELECT * FROM intentions WHERE domain_id = $1 ORDER BY name")
        .bind(id::encode(domain_id))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie des attributs d'une intention.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle.
pub async fn update_intention(
    pool: &Pool,
    intention_id: &ID,
    changeset: IntentionChangeset,
) -> Result<Intention, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE intentions SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(domain_id) = changeset.domain_id {
        set.push("domain_id = ")
            .push_bind_unseparated(domain_id.as_bytes().to_vec());
    }
    if let Some(name) = changeset.name {
        set.push("name = ").push_bind_unseparated(name);
    }
    if let Some(agent_context) = changeset.agent_context {
        set.push("agent_context = ")
            .push_bind_unseparated(agent_context);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(intention_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime une intention.
pub async fn delete_intention(pool: &Pool, intention_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM intentions WHERE id = $1")
        .bind(id::encode(intention_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

/// Liste les intentions actuellement associées à un projet d'acte légal.
pub async fn list_intentions_for_legal_act(
    pool: &Pool,
    legal_act_id: &ID,
) -> Result<Vec<Intention>, StorageError> {
    let rows = sqlx::query(
        "SELECT i.* FROM intentions i \
         INNER JOIN legal_act_intentions lai ON lai.intention_id = i.id \
         WHERE lai.legal_act_id = $1 ORDER BY i.name",
    )
    .bind(id::encode(legal_act_id))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(from_row).collect()
}

/// Associe une intention à un projet d'acte légal.
pub async fn add_intention_to_legal_act(
    pool: &Pool,
    legal_act_id: &ID,
    intention_id: &ID,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO legal_act_intentions (legal_act_id, intention_id) VALUES ($1, $2) \
         ON CONFLICT DO NOTHING",
    )
    .bind(id::encode(legal_act_id))
    .bind(id::encode(intention_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Retire une intention d'un projet d'acte légal.
pub async fn remove_intention_from_legal_act(
    pool: &Pool,
    legal_act_id: &ID,
    intention_id: &ID,
) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM legal_act_intentions WHERE legal_act_id = $1 AND intention_id = $2")
        .bind(id::encode(legal_act_id))
        .bind(id::encode(intention_id))
        .execute(pool)
        .await?;
    Ok(())
}
