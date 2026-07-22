use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Accès configuré à l'API GéoRisques (`agent::tools::GeorisquesClient`),
/// gérable depuis `/admin/integrations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeorisquesCredentials {
    /// Clé API chiffrée au repos avec `marie::secret::SecretManager` (voir
    /// `app::pages::admin::integrations::encrypt_credential`) : un
    /// `marie::secret::EncryptedSecret` sérialisé en JSON, pas directement
    /// le résultat de `shared::crypto::encrypt`. `None` si non configurée.
    /// Le déchiffrement se fait via `agent::tools::secret::decrypt`.
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
    /// Secret client chiffré au repos avec `marie::secret::SecretManager`
    /// (voir la doc de [`GeorisquesCredentials::api_key_encrypted`], même
    /// convention de stockage). Déchiffré via `agent::tools::secret::decrypt`.
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
