use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{CreateUser, User, UserChangeset};

pub(crate) fn from_row(row: PgRow) -> Result<User, StorageError> {
    Ok(User {
        id: id::column(&row, "id")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        suspended_at: row.try_get("suspended_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée un compte utilisateur.
pub async fn create_user(pool: &Pool, args: CreateUser) -> Result<User, StorageError> {
    let new_id = shared::id::generate_id();
    let row =
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3) RETURNING *")
            .bind(id::encode(&new_id))
            .bind(args.email)
            .bind(args.display_name)
            .fetch_one(pool)
            .await?;
    from_row(row)
}

/// Récupère un utilisateur par son identifiant.
pub async fn get_user(pool: &Pool, user_id: &ID) -> Result<User, StorageError> {
    let row = sqlx::query("SELECT * FROM users WHERE id = $1")
        .bind(id::encode(user_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Récupère un utilisateur par son adresse email.
pub async fn get_user_by_email(pool: &Pool, email: &str) -> Result<User, StorageError> {
    let row = sqlx::query("SELECT * FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Met à jour tout ou partie du profil d'un utilisateur.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle.
pub async fn update_user(
    pool: &Pool,
    user_id: &ID,
    changeset: UserChangeset,
) -> Result<User, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE users SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(email) = changeset.email {
        set.push("email = ").push_bind_unseparated(email);
    }
    if let Some(display_name) = changeset.display_name {
        set.push("display_name = ")
            .push_bind_unseparated(display_name);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(user_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Suspend un compte utilisateur sans le supprimer.
pub async fn suspend_user(pool: &Pool, user_id: &ID) -> Result<User, StorageError> {
    let row = sqlx::query(
        "UPDATE users SET suspended_at = now(), updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id::encode(user_id))
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Réactive un compte utilisateur préalablement suspendu.
pub async fn reactivate_user(pool: &Pool, user_id: &ID) -> Result<User, StorageError> {
    let row = sqlx::query(
        "UPDATE users SET suspended_at = NULL, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id::encode(user_id))
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime définitivement un compte utilisateur.
///
/// Échoue si l'utilisateur possède un historique référencé (ex. `legal_act_updates`) ;
/// préférer [`suspend_user`] dans ce cas conformément à la contrainte métier.
pub async fn delete_user(pool: &Pool, user_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id::encode(user_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

/// Liste l'ensemble des comptes utilisateurs.
pub async fn list_users(pool: &Pool) -> Result<Vec<User>, StorageError> {
    let rows = sqlx::query("SELECT * FROM users ORDER BY display_name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}
