//! Outils `ask_user` et `request_document`, qui délèguent à l'application
//! hôte via [`UserInteractionPort`] et [`DocumentRequestPort`] : ce crate ne
//! sait rien de la session ou de l'UI, seulement comment formuler la
//! demande et interpréter la réponse.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::ToolError,
    ports::{DocumentRequestPort, UserInteractionPort},
    tool::{Tool, ToolOutput},
};

#[derive(Deserialize)]
struct AskUserArguments {
    question: String,
}

/// Outil `ask_user` : pose une question ou demande une confirmation à
/// l'inspecteur.
pub struct AskUserTool {
    user_interaction: Arc<dyn UserInteractionPort>,
}

impl AskUserTool {
    #[must_use]
    pub fn new(user_interaction: Arc<dyn UserInteractionPort>) -> Self {
        Self { user_interaction }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Pose une question ou demande une confirmation à l'inspecteur en charge de l'acte."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "Question posée à l'utilisateur" }
            },
            "required": ["question"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: AskUserArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let answer = self.user_interaction.ask(&args.question).await?;
        Ok(ToolOutput::new(answer))
    }
}

#[derive(Deserialize)]
struct RequestDocumentArguments {
    prompt: String,
    #[serde(default)]
    accepted_mime_types: Vec<String>,
}

/// Outil `request_document` : demande un document externe à l'utilisateur
/// (upload).
pub struct RequestDocumentTool {
    document_request: Arc<dyn DocumentRequestPort>,
}

impl RequestDocumentTool {
    #[must_use]
    pub fn new(document_request: Arc<dyn DocumentRequestPort>) -> Self {
        Self { document_request }
    }
}

#[async_trait]
impl Tool for RequestDocumentTool {
    fn name(&self) -> &str {
        "request_document"
    }

    fn description(&self) -> &str {
        "Demande à l'utilisateur de fournir un document externe (upload)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Description du document demandé" },
                "accepted_mime_types": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Types MIME acceptés (ex: \"application/pdf\")"
                }
            },
            "required": ["prompt"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: RequestDocumentArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let document = self.document_request.request_document(&args.prompt, &args.accepted_mime_types).await?;
        let output = serde_json::to_string(&document)
            .map_err(|error| ToolError::Other(format!("échec de sérialisation du document : {error}")))?;
        Ok(ToolOutput::new(output))
    }
}
