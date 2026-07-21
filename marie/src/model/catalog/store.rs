use serde::{Deserialize, Serialize};

use crate::{
    model::{
        catalog::ModelId,
        declaration::{EncryptedModelDeclaration, ModelDeclaration},
    },
    persistency::store::Persisted,
    secret::{SecretManager, SecretResult},
};

/// Représentation persistée d'une entrée du catalogue (voir
/// `network::cp::state::ControlPlaneStateMachineStore`) : `id` est porté par
/// la valeur elle-même (pas seulement par la clé de stockage), pour permettre
/// [`crate::persistency::store::Store::list`] de reconstituer le catalogue
/// complet à froid sans avoir à re-parser les clés `redb`. `declaration` a
/// déjà sa clé API chiffrée (voir [`encrypt_for_storage`]) — jamais en clair
/// sur disque.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredModel {
    pub id: ModelId,
    pub declaration: EncryptedModelDeclaration,
}

impl Persisted for StoredModel {
    type Id = ModelId;

    const NAMESPACE: &'static str = "model";

    fn encode(&self) -> Vec<u8> {
        // Uniquement des `String`/`Vec<u8>` : la sérialisation JSON ne peut
        // pas échouer en pratique (même choix que `RpcCall::new`).
        serde_json::to_vec(self).unwrap()
    }

    fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// Chiffre `declaration` pour stockage au repos (voir [`StoredModel`]) : la
/// clé API est chiffrée avec [`SecretManager::derive_storage_key`], une clé
/// stable dérivée de la master key du cluster — contrairement à
/// [`SecretManager::derive_node_key`], elle ne dépend pas d'un `PeerId`
/// (régénéré à chaque démarrage, voir `network::cp::derive_node_id`), donc un
/// nœud peut déchiffrer à froid ce qu'il a persisté lors d'un précédent
/// démarrage.
pub fn encrypt_for_storage(declaration: &ModelDeclaration, secret: &SecretManager) -> SecretResult<EncryptedModelDeclaration> {
    let storage_key = secret.derive_storage_key()?;
    let api_key = secret.encrypt_api_key(&declaration.api_key, &storage_key)?;
    Ok(declaration.encrypt(api_key))
}

/// Déchiffre une déclaration lue depuis le stockage local (voir
/// [`encrypt_for_storage`]).
pub fn decrypt_from_storage(encrypted: &EncryptedModelDeclaration, secret: &SecretManager) -> SecretResult<ModelDeclaration> {
    let storage_key = secret.derive_storage_key()?;
    let api_key = secret.decrypt_api_key(&encrypted.api_key, &storage_key)?;
    Ok(encrypted.clone().into_declaration(api_key))
}
