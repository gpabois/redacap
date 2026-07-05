use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;

/// Rattache un utilisateur à un groupe.
pub async fn add_user_to_group(
    pool: &Pool,
    user_id: &ID,
    group_id: &ID,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO user_groups (user_id, group_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(id::encode(user_id))
    .bind(id::encode(group_id))
    .execute(pool)
    .await?;
    Ok(())
}

/// Détache un utilisateur d'un groupe.
pub async fn remove_user_from_group(
    pool: &Pool,
    user_id: &ID,
    group_id: &ID,
) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM user_groups WHERE user_id = $1 AND group_id = $2")
        .bind(id::encode(user_id))
        .bind(id::encode(group_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

/// Liste les identifiants des groupes directs d'un utilisateur.
pub async fn list_groups_for_user(pool: &Pool, user_id: &ID) -> Result<Vec<ID>, StorageError> {
    let rows = sqlx::query("SELECT group_id FROM user_groups WHERE user_id = $1")
        .bind(id::encode(user_id))
        .fetch_all(pool)
        .await?;
    rows.iter().map(|row| id::column(row, "group_id")).collect()
}

/// Liste les identifiants des membres directs d'un groupe.
pub async fn list_users_in_group(pool: &Pool, group_id: &ID) -> Result<Vec<ID>, StorageError> {
    let rows = sqlx::query("SELECT user_id FROM user_groups WHERE group_id = $1")
        .bind(id::encode(group_id))
        .fetch_all(pool)
        .await?;
    rows.iter().map(|row| id::column(row, "user_id")).collect()
}
