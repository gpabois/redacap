//! Outils `read_metadata` et `write_metadata`, qui délèguent à
//! l'application hôte via [`MetadataPort`].

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::ToolError,
    ports::MetadataPort,
    tool::{Tool, ToolOutput},
};

#[derive(Deserialize)]
struct ReadMetadataArguments {
    key: String,
}

/// Outil `read_metadata` : lit les métadonnées contextuelles de l'acte en
/// cours.
pub struct ReadMetadataTool {
    metadata: Arc<dyn MetadataPort>,
}

impl ReadMetadataTool {
    #[must_use]
    pub fn new(metadata: Arc<dyn MetadataPort>) -> Self {
        Self { metadata }
    }
}

#[async_trait]
impl Tool for ReadMetadataTool {
    fn name(&self) -> &str {
        "read_metadata"
    }

    fn description(&self) -> &str {
        "Lit les métadonnées contextuelles de l'acte en cours (installation, rubriques ICPE, émissaires...)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "key": { "type": "string", "description": "Clé de la métadonnée à lire" }
            },
            "required": ["key"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: ReadMetadataArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let value = self.metadata.read(&args.key).await?;
        Ok(ToolOutput::new(value.unwrap_or(Value::Null).to_string()))
    }
}

#[derive(Deserialize)]
struct WriteMetadataArguments {
    key: String,
    value: Value,
}

/// Outil `write_metadata` : met à jour les métadonnées contextuelles. Les
/// clés critiques doivent être déclarées via [`Tool::requires_confirmation`]
/// dans la configuration du registre, l'agent ne validant jamais une
/// modification de métadonnées critique sans confirmation explicite.
pub struct WriteMetadataTool {
    metadata: Arc<dyn MetadataPort>,
    requires_confirmation: bool,
}

impl WriteMetadataTool {
    #[must_use]
    pub fn new(metadata: Arc<dyn MetadataPort>, requires_confirmation: bool) -> Self {
        Self { metadata, requires_confirmation }
    }
}

#[async_trait]
impl Tool for WriteMetadataTool {
    fn name(&self) -> &str {
        "write_metadata"
    }

    fn description(&self) -> &str {
        "Met à jour les métadonnées contextuelles de l'acte en cours."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "key": { "type": "string", "description": "Clé de la métadonnée à écrire" },
                "value": { "description": "Nouvelle valeur de la métadonnée" }
            },
            "required": ["key", "value"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        self.requires_confirmation
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: WriteMetadataArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        self.metadata.write(&args.key, args.value).await?;
        Ok(ToolOutput::new("métadonnée mise à jour"))
    }
}
