use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::secret::EncryptedSecret;

/// Identifiant unique d'un modèle dans le [`ModelCatalog`](crate::model::catalog::ModelCatalog).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModelId(String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ModelId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for ModelId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl Borrow<str> for ModelId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDeclaration {
    pub base_url: String,
    pub client_id: String,
    pub api_key: String,
    pub model: String,
    /// Prompt système appliqué par défaut à tout agent utilisant ce modèle.
    /// `None` si le modèle n'en définit pas (l'appelant fournit alors son
    /// propre contexte système, voir [`crate::agent::context::Context`]).
    pub system_prompt: Option<String>,
}

impl ModelDeclaration {
    /// Produit la représentation chiffrée de cette déclaration, destinée à
    /// transiter sur le réseau (voir [`RpcCall::GET_MODEL`](crate::network::cp::rpc::RpcCall::GET_MODEL))
    /// ou à être persistée au repos (voir `model::catalog::store`) :
    /// `api_key` doit déjà avoir été chiffrée pour le destinataire (voir
    /// `SecretManager::encrypt_api_key`), jamais en clair.
    #[must_use]
    pub fn encrypt(&self, api_key: EncryptedSecret) -> EncryptedModelDeclaration {
        EncryptedModelDeclaration {
            base_url: self.base_url.clone(),
            client_id: self.client_id.clone(),
            api_key,
            model: self.model.clone(),
            system_prompt: self.system_prompt.clone(),
        }
    }
}

/// Représentation d'un [`ModelDeclaration`] telle qu'elle transite entre le
/// control plane et un nœud consommateur (voir `RpcCall::GET_MODEL`) : la clé
/// API n'y est jamais en clair, seulement chiffrée pour le nœud destinataire
/// (voir `SecretManager::derive_node_key` côté control plane et
/// `NetworkClient::decrypt_secret` côté consommateur).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedModelDeclaration {
    pub base_url: String,
    pub client_id: String,
    pub api_key: EncryptedSecret,
    pub model: String,
    pub system_prompt: Option<String>,
}

impl EncryptedModelDeclaration {
    /// Reconstitue la déclaration en clair une fois `api_key` déchiffrée
    /// localement (voir `NetworkClient::decrypt_secret` ou
    /// `model::catalog::store::decrypt_from_storage`).
    #[must_use]
    pub fn into_declaration(self, api_key: String) -> ModelDeclaration {
        ModelDeclaration {
            base_url: self.base_url,
            client_id: self.client_id,
            api_key,
            model: self.model,
            system_prompt: self.system_prompt,
        }
    }
}
