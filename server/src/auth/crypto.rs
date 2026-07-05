//! Déchiffrement des `client_secret` des fournisseurs OIDC au repos, avec la
//! clé dérivée de `SECRET_ENCRYPTION_KEY` (voir `AppState::secret_encryption_key`).
//!
//! Le chiffrement/déchiffrement lui-même (AES-256-GCM, convention nonce(12)
//! || texte chiffré) vit dans `shared::crypto` : `app` (panneau
//! administrateur, `/admin/oidc`) doit pouvoir chiffrer un nouveau secret
//! mais ne peut pas dépendre de `server` (dépendance inverse, `server`
//! dépend déjà de `app`). Ce module ne fait donc que traduire l'erreur vers
//! [`AuthError`]. Les autres consommateurs de `SECRET_ENCRYPTION_KEY` (modèles
//! IA, intégrations GéoRisques/Légifrance) appellent `shared::crypto::decrypt`
//! directement depuis `server::editor::ws`, car leurs erreurs ne relèvent pas
//! du domaine [`AuthError`].

use super::AuthError;

/// Déchiffre un `client_secret` chiffré selon la convention de
/// `shared::crypto` (nonce 12 octets || texte chiffré).
pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<String, AuthError> {
    shared::crypto::decrypt(key, data).map_err(|_| AuthError::Crypto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let key = [7u8; 32];
        let ciphertext =
            shared::crypto::encrypt(&key, "un-secret-client-oidc").expect("chiffrement réussi");
        assert_eq!(decrypt(&key, &ciphertext).unwrap(), "un-secret-client-oidc");
    }

    #[test]
    fn decrypt_rejects_wrong_key() {
        let key = [7u8; 32];
        let other_key = [9u8; 32];
        let ciphertext =
            shared::crypto::encrypt(&key, "un-secret-client-oidc").expect("chiffrement réussi");
        assert!(decrypt(&other_key, &ciphertext).is_err());
    }
}
