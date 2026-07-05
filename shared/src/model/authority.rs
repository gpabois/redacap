use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Autorité administrative référentielle (ex. DREAL, préfecture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Authority {
    pub id: ID,
    pub nom: String,
    pub code: String,
    pub logo_url: Option<String>,
    pub tutelle: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'une autorité administrative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateAuthority {
    pub nom: String,
    pub code: String,
    pub logo_url: Option<String>,
    pub tutelle: Option<String>,
}

/// Attributs modifiables d'une autorité administrative.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle. `logo_url`/`tutelle` sont donc
/// doublement optionnels : `None` (le champ) = inchangé, `Some(None)` = effacé,
/// `Some(Some(valeur))` = remplacé.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorityChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutelle: Option<Option<String>>,
}
