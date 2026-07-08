//! Outils `ask_user`, `ask_questions` et `request_document` : contrairement
//! aux autres outils, ils ne s'exécutent jamais eux-mêmes (leur [`Tool::call`]
//! n'est jamais invoqué). Ils se contentent de valider leurs arguments et de
//! les traduire en [`PauseRequest`] via [`Tool::pause_request`] : c'est
//! l'orchestrateur (voir `crate::orchestration`) qui suspend l'exécution du
//! frame courant et persiste la demande, plutôt que d'attendre la réponse de
//! l'utilisateur en bloquant — indispensable pour qu'une pause survive à une
//! déconnexion ou un redémarrage du serveur.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::ToolError,
    ports::Question,
    tool::{PauseRequest, Tool, ToolOutput},
};

#[derive(Deserialize)]
struct AskUserArguments {
    question: String,
}

/// Outil `ask_user` : pose une question ou demande une confirmation à
/// l'inspecteur.
pub struct AskUserTool;

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

    fn pause_request(&self, arguments: &Value) -> Result<Option<PauseRequest>, ToolError> {
        let args: AskUserArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        Ok(Some(PauseRequest::Ask {
            question: args.question,
        }))
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        unreachable!(
            "ask_user est reconnu via Tool::pause_request : l'orchestrateur ne doit jamais \
             appeler call() dessus"
        )
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
pub struct AskQuestionsTool;

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

    fn pause_request(&self, arguments: &Value) -> Result<Option<PauseRequest>, ToolError> {
        let args: AskQuestionsArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let questions: Vec<Question> = args
            .questions
            .into_iter()
            .map(|q| Question {
                id: q.id,
                label: q.label,
                options: q.options,
            })
            .collect();

        Ok(Some(PauseRequest::AskQuestions {
            prompt: args.prompt,
            questions,
        }))
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        unreachable!(
            "ask_questions est reconnu via Tool::pause_request : l'orchestrateur ne doit jamais \
             appeler call() dessus"
        )
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
pub struct RequestDocumentTool;

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

    fn pause_request(&self, arguments: &Value) -> Result<Option<PauseRequest>, ToolError> {
        let args: RequestDocumentArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        Ok(Some(PauseRequest::RequestDocument {
            prompt: args.prompt,
            accepted_mime_types: args.accepted_mime_types,
        }))
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        unreachable!(
            "request_document est reconnu via Tool::pause_request : l'orchestrateur ne doit \
             jamais appeler call() dessus"
        )
    }
}
