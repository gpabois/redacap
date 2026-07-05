use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Accès configuré à l'API GéoRisques (`agent::tools::GeorisquesClient`),
/// gérable depuis `/admin/integrations`. L'API `v1` est accessible sans
/// jeton : `api_key_encrypted` est optionnelle, elle ne fait qu'augmenter le
/// quota de requêtes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeorisquesCredentials {
    /// Clé API chiffrée au repos ; `None` si non configurée. Le déchiffrement
    /// est réservé à `server`.
    pub api_key_encrypted: Option<Vec<u8>>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<ID>,
}

/// Attributs à enregistrer pour la configuration GéoRisques (upsert).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetGeorisquesCredentials {
    pub api_key_encrypted: Option<Vec<u8>>,
    pub updated_by: Option<ID>,
}

/// Accès configuré à l'API Légifrance (`agent::tools::LegifranceClient`),
/// gérable depuis `/admin/integrations`. Authentification OAuth2
/// `client_credentials` du portail PISTE : `client_id` et `client_secret`
/// sont tous deux requis pour que les outils `legifrance_search`/
/// `legifrance_fetch` soient disponibles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegifranceCredentials {
    pub client_id: Option<String>,
    /// Secret client chiffré au repos ; le déchiffrement est réservé à `server`.
    pub client_secret_encrypted: Option<Vec<u8>>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<ID>,
}

/// Attributs à enregistrer pour la configuration Légifrance (upsert).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLegifranceCredentials {
    pub client_id: Option<String>,
    pub client_secret_encrypted: Option<Vec<u8>>,
    pub updated_by: Option<ID>,
}
