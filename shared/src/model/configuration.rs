use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Paramètre applicatif global stocké en clé/valeur (ex. `agent.system_prompt`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<ID>,
}

/// Attributs nécessaires à la création d'un paramètre de configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateConfiguration {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_by: Option<ID>,
}

/// Attributs modifiables d'un paramètre de configuration existant.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle. `updated_by` est donc
/// doublement optionnel : `None` (le champ) = inchangé, `Some(None)` = effacé,
/// `Some(Some(id))` = remplacé.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<Option<ID>>,
}
