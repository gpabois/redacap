use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Fournisseur OpenID Connect autorisé pour l'authentification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OidcProvider {
    pub id: ID,
    pub name: String,
    pub issuer_url: String,
    pub client_id: String,
    /// Secret client chiffré au repos ; le déchiffrement est réservé à `server`.
    pub client_secret_encrypted: Vec<u8>,
    pub scopes: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à l'enregistrement d'un fournisseur OpenID Connect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateOidcProvider {
    pub name: String,
    pub issuer_url: String,
    pub client_id: String,
    /// Secret client déjà chiffré au repos ; le déchiffrement est réservé à `server`.
    pub client_secret_encrypted: Vec<u8>,
    pub scopes: Vec<String>,
}

/// Attributs modifiables d'un fournisseur OpenID Connect.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OidcProviderChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_encrypted: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}
