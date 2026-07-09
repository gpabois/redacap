use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

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
    pub model: String
}
