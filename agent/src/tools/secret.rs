use marie::secret::{EncryptedSecret, SecretCodec, SecretManager};

/// Déchiffre un secret persisté par `storage::external_credentials`
/// (colonnes `*_encrypted`), chiffré au repos avec
/// `SecretManager::derive_storage_key` — même principe que
/// `marie::model::catalog::store::StoredModel::{encrypt,decrypt}`, appliqué
/// à un secret isolé (clé API, client secret) plutôt qu'à un modèle entier.
/// `encrypted` est un [`EncryptedSecret`] sérialisé en JSON (voir le pendant
/// côté écriture dans `app::pages::admin::integrations`).
pub(crate) fn decrypt(secret: &SecretManager, encrypted: &[u8]) -> anyhow::Result<String> {
    let encrypted: EncryptedSecret = serde_json::from_slice(encrypted)?;
    let storage_key = secret.derive_storage_key_for_epoch(encrypted.key_epoch)?;
    Ok(storage_key.decrypt_str(encrypted)?)
}
