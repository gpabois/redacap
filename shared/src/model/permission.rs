use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Action réservée : permet toutes les actions sauf retirer les droits d'un
/// autre `administrateur` ou d'un `super administrateur` (voir `Claude.md`
/// § Autorisation).
pub const ACTION_ADMINISTRATEUR: &str = "administrateur";
/// Action réservée : permet toutes les actions, y compris assigner et
/// retirer les droits `administrateur` et `super administrateur`.
pub const ACTION_SUPER_ADMINISTRATEUR: &str = "super_administrateur";

/// `resource_type` conventionnel des permissions globales sans ressource
/// applicative précise (ex. `administrateur`/`super_administrateur`).
pub const RESOURCE_TYPE_APPLICATION: &str = "application";

/// Titulaire d'une permission : un utilisateur ou un groupe, jamais les deux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subject {
    User(ID),
    Group(ID),
}

/// Portée de la ressource ciblée par une permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceScope {
    /// Droit sur une ressource précise.
    Specific(ID),
    /// Droit sur toute ressource gérée par un groupe.
    ManagedByGroup(ID),
    /// Droit global, non circonscrit à une ressource (ex. `super_administrateur`).
    Global,
}

/// Permission triplet `(subject, resource, action)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub id: ID,
    pub subject: Subject,
    pub resource_type: String,
    pub resource: ResourceScope,
    pub action: String,
    pub created_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'une permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePermission {
    pub subject: Subject,
    pub resource_type: String,
    pub resource: ResourceScope,
    pub action: String,
}

/// Attributs modifiables d'une permission existante.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}
