use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use sqlx::Row;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;

/// Enregistre le mot de passe d'un utilisateur, en remplaçant le précédent le cas échéant.
///
/// Le mot de passe en clair n'est jamais persisté : seul le hash Argon2 (qui embarque
/// son propre sel aléatoire) est stocké.
pub async fn set_password(pool: &Pool, user_id: &ID, password: &str) -> Result<(), StorageError> {
    let password_hash = hash_password(password)?;
    sqlx::query(
        "INSERT INTO credentials (user_id, password_hash) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET password_hash = EXCLUDED.password_hash, updated_at = now()",
    )
    .bind(id::encode(user_id))
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// Vérifie qu'un mot de passe en clair correspond au hash stocké pour l'utilisateur.
///
/// Renvoie `false` (plutôt qu'une erreur) si l'utilisateur ne possède pas d'identifiants
/// par mot de passe, afin de ne pas distinguer ce cas d'un mot de passe erroné.
pub async fn verify_password(
    pool: &Pool,
    user_id: &ID,
    password: &str,
) -> Result<bool, StorageError> {
    let row = sqlx::query("SELECT password_hash FROM credentials WHERE user_id = $1")
        .bind(id::encode(user_id))
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let stored_hash: String = row.try_get("password_hash")?;
    Ok(verify_password_hash(&stored_hash, password))
}

/// Supprime les identifiants par mot de passe d'un utilisateur.
pub async fn delete_credential(pool: &Pool, user_id: &ID) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM credentials WHERE user_id = $1")
        .bind(id::encode(user_id))
        .execute(pool)
        .await?;
    Ok(())
}

/// Indique si l'utilisateur possède des identifiants par mot de passe.
pub async fn has_credential(pool: &Pool, user_id: &ID) -> Result<bool, StorageError> {
    let row = sqlx::query("SELECT 1 AS present FROM credentials WHERE user_id = $1")
        .bind(id::encode(user_id))
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub(crate) fn hash_password(password: &str) -> Result<String, StorageError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| StorageError::Credential(err.to_string()))
}

fn verify_password_hash(stored_hash: &str, password: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed_hash) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let hash = hash_password("un-mot-de-passe-robuste").expect("hachage réussi");
        assert!(verify_password_hash(&hash, "un-mot-de-passe-robuste"));
    }

    #[test]
    fn verify_rejects_wrong_password() {
        let hash = hash_password("un-mot-de-passe-robuste").expect("hachage réussi");
        assert!(!verify_password_hash(&hash, "un-autre-mot-de-passe"));
    }
}
