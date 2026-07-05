use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreateGroup, Group, GroupChangeset};

fn from_row(row: PgRow) -> Result<Group, StorageError> {
    Ok(Group {
        id: id::column(&row, "id")?,
        parent_group_id: id::column_opt(&row, "parent_group_id")?,
        name: row.try_get("name")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée un groupe, éventuellement rattaché à un groupe parent.
pub async fn create_group(pool: &Pool, args: CreateGroup) -> Result<Group, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO groups (id, parent_group_id, name) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.parent_group_id.as_ref().map(id::encode))
    .bind(args.name)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère un groupe par son identifiant.
pub async fn get_group(pool: &Pool, group_id: &ID) -> Result<Group, StorageError> {
    let row = sqlx::query("SELECT * FROM groups WHERE id = $1")
        .bind(id::encode(group_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste l'ensemble des groupes, tous niveaux confondus.
pub async fn list_all_groups(pool: &Pool) -> Result<Vec<Group>, StorageError> {
    let rows = sqlx::query("SELECT * FROM groups ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Liste l'ensemble des descendants d'un groupe (sous-groupes, sous-sous-groupes...).
///
/// Utilisé notamment pour la résolution des permissions effectives (une entité
/// hérite des droits de ses descendants, cf. `Claude.md` § Autorisation) et pour
/// empêcher qu'un groupe soit déplacé sous l'un de ses propres descendants.
pub async fn list_descendant_groups(
    pool: &Pool,
    group_id: &ID,
) -> Result<Vec<Group>, StorageError> {
    let mut descendants = Vec::new();
    let mut frontier = vec![*group_id];
    while let Some(current_id) = frontier.pop() {
        let children = list_child_groups(pool, Some(&current_id)).await?;
        for child in children {
            frontier.push(child.id);
            descendants.push(child);
        }
    }
    Ok(descendants)
}

/// Liste les sous-groupes directs d'un groupe (ou les groupes racine si `None`).
pub async fn list_child_groups(
    pool: &Pool,
    parent_group_id: Option<&ID>,
) -> Result<Vec<Group>, StorageError> {
    let rows = match parent_group_id {
        Some(parent_id) => {
            sqlx::query("SELECT * FROM groups WHERE parent_group_id = $1 ORDER BY name")
                .bind(id::encode(parent_id))
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query("SELECT * FROM groups WHERE parent_group_id IS NULL ORDER BY name")
                .fetch_all(pool)
                .await?
        }
    };
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie des attributs d'un groupe.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle. `parent_group_id: Some(None)` détache le groupe de son parent.
pub async fn update_group(
    pool: &Pool,
    group_id: &ID,
    changeset: GroupChangeset,
) -> Result<Group, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE groups SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(name) = changeset.name {
        set.push("name = ").push_bind_unseparated(name);
    }
    if let Some(parent_group_id) = changeset.parent_group_id {
        set.push("parent_group_id = ")
            .push_bind_unseparated(parent_group_id.map(|id| id.as_bytes().to_vec()));
    }
    builder
        .push(" WHERE id = ")
        .push_bind(group_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime un groupe.
///
/// Les sous-groupes sont détachés (`parent_group_id` mis à `NULL`) et les
/// permissions/rattachements portés par ce groupe sont propagés en cascade.
pub async fn delete_group(pool: &Pool, group_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM groups WHERE id = $1")
        .bind(id::encode(group_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
