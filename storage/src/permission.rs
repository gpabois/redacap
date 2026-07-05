use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreatePermission, Permission, PermissionChangeset, ResourceScope, Subject};

fn from_row(row: PgRow) -> Result<Permission, StorageError> {
    let subject = match (
        id::column_opt(&row, "subject_user_id")?,
        id::column_opt(&row, "subject_group_id")?,
    ) {
        (Some(user_id), None) => Subject::User(user_id),
        (None, Some(group_id)) => Subject::Group(group_id),
        _ => {
            return Err(StorageError::InvalidId(
                "titulaire de permission ambigu".into(),
            ));
        }
    };
    let resource = match (
        id::column_opt(&row, "resource_id")?,
        id::column_opt(&row, "resource_group_id")?,
    ) {
        (Some(resource_id), None) => ResourceScope::Specific(resource_id),
        (None, Some(group_id)) => ResourceScope::ManagedByGroup(group_id),
        (None, None) => ResourceScope::Global,
        (Some(_), Some(_)) => {
            return Err(StorageError::InvalidId(
                "portée de ressource ambiguë".into(),
            ));
        }
    };
    Ok(Permission {
        id: id::column(&row, "id")?,
        subject,
        resource_type: row.try_get("resource_type")?,
        resource,
        action: row.try_get("action")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Crée une permission pour un titulaire et une portée de ressource donnés.
pub async fn create_permission(
    pool: &Pool,
    args: CreatePermission,
) -> Result<Permission, StorageError> {
    let new_id = shared::id::generate_id();
    let (subject_user_id, subject_group_id) = match args.subject {
        Subject::User(user_id) => (Some(user_id), None),
        Subject::Group(group_id) => (None, Some(group_id)),
    };
    let (resource_id, resource_group_id) = match args.resource {
        ResourceScope::Specific(resource_id) => (Some(resource_id), None),
        ResourceScope::ManagedByGroup(group_id) => (None, Some(group_id)),
        ResourceScope::Global => (None, None),
    };
    let row = sqlx::query(
        "INSERT INTO permissions \
         (id, subject_user_id, subject_group_id, resource_type, resource_id, resource_group_id, action) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(subject_user_id.as_ref().map(id::encode))
    .bind(subject_group_id.as_ref().map(id::encode))
    .bind(args.resource_type)
    .bind(resource_id.as_ref().map(id::encode))
    .bind(resource_group_id.as_ref().map(id::encode))
    .bind(args.action)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère une permission par son identifiant.
pub async fn get_permission(pool: &Pool, permission_id: &ID) -> Result<Permission, StorageError> {
    let row = sqlx::query("SELECT * FROM permissions WHERE id = $1")
        .bind(id::encode(permission_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste les permissions directement accordées à un utilisateur.
pub async fn list_permissions_for_user(
    pool: &Pool,
    user_id: &ID,
) -> Result<Vec<Permission>, StorageError> {
    let rows = sqlx::query("SELECT * FROM permissions WHERE subject_user_id = $1")
        .bind(id::encode(user_id))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Liste les permissions directement accordées à un groupe.
pub async fn list_permissions_for_group(
    pool: &Pool,
    group_id: &ID,
) -> Result<Vec<Permission>, StorageError> {
    let rows = sqlx::query("SELECT * FROM permissions WHERE subject_group_id = $1")
        .bind(id::encode(group_id))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie de la ressource et de l'action ciblées par une permission.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent leur
/// valeur actuelle. `resource` remplace toujours `resource_id`/`resource_group_id` ensemble,
/// les deux colonnes étant mutuellement exclusives.
pub async fn update_permission(
    pool: &Pool,
    permission_id: &ID,
    changeset: PermissionChangeset,
) -> Result<Permission, StorageError> {
    if changeset.resource_type.is_none()
        && changeset.resource.is_none()
        && changeset.action.is_none()
    {
        return get_permission(pool, permission_id).await;
    }

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE permissions SET ");
    let mut set = builder.separated(", ");
    if let Some(resource_type) = changeset.resource_type {
        set.push("resource_type = ")
            .push_bind_unseparated(resource_type);
    }
    if let Some(resource) = changeset.resource {
        let (resource_id, resource_group_id) = match resource {
            ResourceScope::Specific(resource_id) => (Some(resource_id), None),
            ResourceScope::ManagedByGroup(group_id) => (None, Some(group_id)),
            ResourceScope::Global => (None, None),
        };
        set.push("resource_id = ")
            .push_bind_unseparated(resource_id.map(|id| id.as_bytes().to_vec()));
        set.push("resource_group_id = ")
            .push_bind_unseparated(resource_group_id.map(|id| id.as_bytes().to_vec()));
    }
    if let Some(action) = changeset.action {
        set.push("action = ").push_bind_unseparated(action);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(permission_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Révoque une permission.
pub async fn delete_permission(pool: &Pool, permission_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM permissions WHERE id = $1")
        .bind(id::encode(permission_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
