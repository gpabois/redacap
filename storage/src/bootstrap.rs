//! État bootstrap : création du compte super administrateur unique lorsque
//! aucun n'existe encore (voir `Claude.md` § « Ajoute un état bootstrap... »).
//!
//! Tant qu'aucune permission globale `super_administrateur` n'existe,
//! `server::guard::bootstrap_guard` redirige toute requête vers `/bootstrap`.
//! [`create_super_administrator`] crée l'utilisateur, ses identifiants et
//! cette permission dans une même transaction, ce qui termine l'état
//! bootstrap.

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::model::{ACTION_SUPER_ADMINISTRATEUR, RESOURCE_TYPE_APPLICATION, User};

/// Clé arbitraire mais stable du verrou consultatif Postgres dédié au
/// bootstrap : sérialise les tentatives concurrentes de
/// [`create_super_administrator`] pour garantir l'unicité du compte créé.
const BOOTSTRAP_ADVISORY_LOCK_KEY: i64 = 0x5245_4441_4341_5030;

/// Indique si l'application est encore en état bootstrap, c'est-à-dire si
/// aucun compte ne détient encore la permission globale
/// `super_administrateur`.
pub async fn is_required(pool: &Pool) -> Result<bool, StorageError> {
    Ok(!has_super_administrator(pool).await?)
}

async fn has_super_administrator(pool: &Pool) -> Result<bool, StorageError> {
    let row = sqlx::query(
        "SELECT 1 AS present FROM permissions \
         WHERE action = $1 AND resource_id IS NULL AND resource_group_id IS NULL LIMIT 1",
    )
    .bind(ACTION_SUPER_ADMINISTRATEUR)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// Crée l'unique compte super administrateur de bootstrap (utilisateur,
/// identifiants par mot de passe, permission globale
/// `super_administrateur`), dans une transaction protégée par un verrou
/// consultatif Postgres.
///
/// Échoue avec [`StorageError::AlreadyBootstrapped`] si un super
/// administrateur existe déjà : l'état bootstrap est alors déjà terminé.
pub async fn create_super_administrator(
    pool: &Pool,
    email: String,
    display_name: String,
    password: &str,
) -> Result<User, StorageError> {
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BOOTSTRAP_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    let already_bootstrapped = sqlx::query(
        "SELECT 1 AS present FROM permissions \
         WHERE action = $1 AND resource_id IS NULL AND resource_group_id IS NULL LIMIT 1",
    )
    .bind(ACTION_SUPER_ADMINISTRATEUR)
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if already_bootstrapped {
        return Err(StorageError::AlreadyBootstrapped);
    }

    let user_id = shared::id::generate_id();
    let row =
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3) RETURNING *")
            .bind(id::encode(&user_id))
            .bind(email)
            .bind(display_name)
            .fetch_one(&mut *tx)
            .await?;
    let user = crate::user::from_row(row)?;

    let password_hash = crate::credential::hash_password(password)?;
    sqlx::query("INSERT INTO credentials (user_id, password_hash) VALUES ($1, $2)")
        .bind(id::encode(&user.id))
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;

    let permission_id = shared::id::generate_id();
    sqlx::query(
        "INSERT INTO permissions (id, subject_user_id, resource_type, action) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id::encode(&permission_id))
    .bind(id::encode(&user.id))
    .bind(RESOURCE_TYPE_APPLICATION)
    .bind(ACTION_SUPER_ADMINISTRATEUR)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(user)
}
