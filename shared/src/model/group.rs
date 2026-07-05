use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Groupe applicatif, pouvant être rattaché à un groupe parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: ID,
    pub parent_group_id: Option<ID>,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'un groupe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateGroup {
    pub name: String,
    pub parent_group_id: Option<ID>,
}

/// Attributs modifiables d'un groupe.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés,
/// les champs à `None` conservent leur valeur actuelle. `parent_group_id` est donc
/// doublement optionnel : `None` (le champ) = inchangé, `Some(None)` = détache du
/// parent, `Some(Some(id))` = rattache au groupe `id`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_group_id: Option<Option<ID>>,
}
