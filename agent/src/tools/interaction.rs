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
    ports::{DocumentRequestPort, Question, UserInteractionPort},
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
struct QuestionSpec {
    id: String,
    label: String,
    #[serde(default)]
    options: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct AskQuestionsArguments {
    prompt: String,
    questions: Vec<QuestionSpec>,
}

/// Outil `ask_questions` : présente un formulaire structuré à l'utilisateur
/// et renvoie ses réponses, en indiquant éventuellement lesquelles ne sont pas
/// satisfaisantes.
pub struct AskQuestionsTool {
    user_interaction: Arc<dyn UserInteractionPort>,
}

impl AskQuestionsTool {
    #[must_use]
    pub fn new(user_interaction: Arc<dyn UserInteractionPort>) -> Self {
        Self { user_interaction }
    }
}

#[async_trait]
impl Tool for AskQuestionsTool {
    fn name(&self) -> &str {
        "ask_questions"
    }

    fn description(&self) -> &str {
        "Présente un formulaire structuré à l'inspecteur et renvoie ses réponses. \
         Chaque réponse peut être marquée comme non satisfaisante avec une raison."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Contexte ou consignes affichés avant le formulaire"
                },
                "questions": {
                    "type": "array",
                    "description": "Liste des questions du formulaire",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Identifiant unique de la question" },
                            "label": { "type": "string", "description": "Libellé affiché à l'utilisateur" },
                            "options": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Si présent, l'utilisateur doit choisir parmi ces options ; sinon réponse libre"
                            }
                        },
                        "required": ["id", "label"]
                    }
                }
            },
            "required": ["prompt", "questions"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: AskQuestionsArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let questions: Vec<Question> = args
            .questions
            .into_iter()
            .map(|q| Question { id: q.id, label: q.label, options: q.options })
            .collect();

        let answers = self.user_interaction.ask_questions(&args.prompt, &questions).await?;

        let output = serde_json::to_string(&answers.iter().map(|a| {
            serde_json::json!({
                "question_id": a.question_id,
                "value": a.value,
                "unsatisfactory_reason": a.unsatisfactory_reason
            })
        }).collect::<Vec<_>>())
        .map_err(|error| ToolError::Other(format!("échec de sérialisation des réponses : {error}")))?;

        Ok(ToolOutput::new(output))
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
