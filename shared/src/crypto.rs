//! Chiffrement symétrique générique (AES-256-GCM), partagé entre `server`
//! (déchiffrement des `client_secret` OIDC à l'authentification) et `app`
//! (chiffrement à l'enregistrement d'un fournisseur OIDC depuis le panneau
//! administrateur). Isolé dans `shared` car `app` ne peut pas dépendre de
//! `server` (dépendance inverse : `server` dépend déjà de `app`).
//!
//! Convention de stockage : les 12 premiers octets sont le nonce, le reste
//! est le texte chiffré (avec tag d'authentification GCM inclus).

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key as AesKey, Nonce};

const NONCE_LEN: usize = 12;

/// Erreur de chiffrement/déchiffrement, sans détail (pour ne pas fournir
/// d'oracle à un attaquant).
#[derive(Debug, thiserror::Error)]
#[error("erreur de chiffrement")]
pub struct CryptoError;

/// Chiffre `plaintext` avec `key`, en préfixant un nonce aléatoire.
pub fn encrypt(key: &[u8], plaintext: &str) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(AesKey::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| CryptoError)?;
    let mut out = nonce.to_vec();
    out.append(&mut ciphertext);
    Ok(out)
}

/// Déchiffre `data` (nonce || texte chiffré) avec `key`.
pub fn decrypt(key: &[u8], data: &[u8]) -> Result<String, CryptoError> {
    if data.len() < NONCE_LEN {
        return Err(CryptoError);
    }
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(AesKey::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| CryptoError)?;
    String::from_utf8(plaintext).map_err(|_| CryptoError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let key = [7u8; 32];
        let ciphertext = encrypt(&key, "un-secret-client-oidc").expect("chiffrement réussi");
        assert_eq!(
            decrypt(&key, &ciphertext).expect("déchiffrement réussi"),
            "un-secret-client-oidc"
        );
    }

    #[test]
    fn decrypt_rejects_wrong_key() {
        let key = [7u8; 32];
        let other_key = [9u8; 32];
        let ciphertext = encrypt(&key, "un-secret-client-oidc").expect("chiffrement réussi");
        assert!(decrypt(&other_key, &ciphertext).is_err());
    }

    #[test]
    fn decrypt_rejects_truncated_data() {
        let key = [7u8; 32];
        assert!(decrypt(&key, &[0u8; 4]).is_err());
    }
}
