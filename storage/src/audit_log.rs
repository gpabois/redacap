use sqlx::Row;
use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{AuditLogEntry, CreateAuditEvent};

fn from_row(row: PgRow) -> Result<AuditLogEntry, StorageError> {
    Ok(AuditLogEntry {
        id: row.try_get("id")?,
        occurred_at: row.try_get("occurred_at")?,
        actor_id: id::column_opt(&row, "actor_id")?,
        actor_ip: row.try_get("actor_ip")?,
        action: row.try_get("action")?,
        resource_type: row.try_get("resource_type")?,
        resource_id: id::column_opt(&row, "resource_id")?,
        details: row.try_get("details")?,
    })
}

/// Trace une action sensible dans le journal d'audit. Écriture append-only : aucune
/// fonction de mise à jour ou de suppression n'est exposée pour ce journal.
pub async fn record_audit_event(
    pool: &Pool,
    args: CreateAuditEvent,
) -> Result<AuditLogEntry, StorageError> {
    let row = sqlx::query(
        "INSERT INTO audit_log (actor_id, actor_ip, action, resource_type, resource_id, details) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(args.actor_id.as_ref().map(id::encode))
    .bind(args.actor_ip)
    .bind(args.action)
    .bind(args.resource_type)
    .bind(args.resource_id.as_ref().map(id::encode))
    .bind(args.details)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère une entrée du journal d'audit par son identifiant.
pub async fn get_audit_event(pool: &Pool, entry_id: i64) -> Result<AuditLogEntry, StorageError> {
    let row = sqlx::query("SELECT * FROM audit_log WHERE id = $1")
        .bind(entry_id)
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste les entrées d'audit concernant une ressource précise, les plus récentes en premier.
pub async fn list_audit_events_for_resource(
    pool: &Pool,
    resource_type: &str,
    resource_id: &ID,
) -> Result<Vec<AuditLogEntry>, StorageError> {
    let rows = sqlx::query(
        "SELECT * FROM audit_log WHERE resource_type = $1 AND resource_id = $2 ORDER BY occurred_at DESC",
    )
    .bind(resource_type)
    .bind(id::encode(resource_id))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(from_row).collect()
}

/// Liste une page d'entrées d'audit, les plus récentes en premier, filtrée
/// optionnellement par type de ressource. Utilisé par le panneau administrateur
/// (`/admin/audit`).
pub async fn list_audit_events(
    pool: &Pool,
    resource_type: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditLogEntry>, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("SELECT * FROM audit_log");
    if let Some(resource_type) = resource_type {
        builder
            .push(" WHERE resource_type = ")
            .push_bind(resource_type);
    }
    builder.push(" ORDER BY occurred_at DESC LIMIT ");
    builder.push_bind(limit);
    builder.push(" OFFSET ");
    builder.push_bind(offset);

    let rows = builder.build().fetch_all(pool).await?;
    rows.into_iter().map(from_row).collect()
}

/// Compte le nombre total d'entrées d'audit, filtré optionnellement par type
/// de ressource. Utilisé pour paginer `list_audit_events`.
pub async fn count_audit_events(
    pool: &Pool,
    resource_type: Option<&str>,
) -> Result<i64, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("SELECT count(*) AS total FROM audit_log");
    if let Some(resource_type) = resource_type {
        builder
            .push(" WHERE resource_type = ")
            .push_bind(resource_type);
    }
    let row = builder.build().fetch_one(pool).await?;
    Ok(row.try_get("total")?)
}

/// Liste les entrées d'audit émises par un acteur donné, les plus récentes en premier.
pub async fn list_audit_events_for_actor(
    pool: &Pool,
    actor_id: &ID,
) -> Result<Vec<AuditLogEntry>, StorageError> {
    let rows = sqlx::query("SELECT * FROM audit_log WHERE actor_id = $1 ORDER BY occurred_at DESC")
        .bind(id::encode(actor_id))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}
