//! Abstraction du modèle de langage utilisé par l'agent. [`LanguageModel`]
//! ne suppose rien d'un fournisseur particulier : [`OpenAiCompatibleModel`]
//! l'implémente pour tout point de terminaison respectant l'API de
//! complétion de chat OpenAI (`POST {base_url}/chat/completions`), ce qui
//! couvre aussi bien les fournisseurs cloud que les passerelles
//! d'agrégation (ex: OpenRouter) ou les serveurs auto-hébergés (Ollama,
//! vLLM, LM Studio...). Changer de modèle revient donc à changer
//! `base_url` / `model` / `api_key`, jamais le code de l'agent.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ModelError;

/// Rôle d'un message dans une conversation avec le modèle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Appel d'outil demandé par le modèle, à exécuter par l'agent.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Message échangé avec le modèle de langage, indépendant du format de
/// transport du fournisseur.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: Some(content.into()), tool_calls: Vec::new(), tool_call_id: None }
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: Some(content.into()), tool_calls: Vec::new(), tool_call_id: None }
    }

    /// Message de réponse contenant des appels d'outils à exécuter (pas de
    /// contenu textuel final).
    #[must_use]
    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self { role: Role::Assistant, content: None, tool_calls, tool_call_id: None }
    }

    /// Résultat d'exécution d'un outil, à renvoyer au modèle.
    #[must_use]
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Description d'un outil exposée au modèle, au format JSON Schema attendu
/// par la plupart des API de "function calling".
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Un fournisseur de modèle de langage capable de produire le prochain
/// message d'une conversation, en tenant compte des outils disponibles.
#[async_trait]
pub trait LanguageModel: Send + Sync {
    /// Nom du modèle interrogé, à des fins de journalisation.
    fn model_name(&self) -> &str;

    async fn complete(&self, messages: &[ChatMessage], tools: &[ToolDefinition]) -> Result<ChatMessage, ModelError>;
}

/// Configuration d'un point de terminaison compatible avec l'API de
/// complétion de chat OpenAI.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleModelConfig {
    /// Racine de l'API, sans le segment `/chat/completions` (ex:
    /// `https://api.openai.com/v1`, `http://localhost:11434/v1`).
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// Client [`LanguageModel`] pour tout point de terminaison compatible avec
/// l'API de complétion de chat OpenAI.
pub struct OpenAiCompatibleModel {
    http: reqwest::Client,
    config: OpenAiCompatibleModelConfig,
}

impl OpenAiCompatibleModel {
    #[must_use]
    pub fn new(config: OpenAiCompatibleModelConfig) -> Self {
        Self { http: reqwest::Client::new(), config }
    }
}

#[async_trait]
impl LanguageModel for OpenAiCompatibleModel {
    fn model_name(&self) -> &str {
        &self.config.model
    }

    async fn complete(&self, messages: &[ChatMessage], tools: &[ToolDefinition]) -> Result<ChatMessage, ModelError> {
        let request = WireRequest {
            model: &self.config.model,
            messages: messages.iter().map(WireMessage::from).collect(),
            tools: tools.iter().map(WireToolDefinition::from).collect(),
        };

        let response = self
            .http
            .post(format!("{}/chat/completions", self.config.base_url.trim_end_matches('/')))
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<WireCompletionResponse>()
            .await?;

        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ModelError::InvalidResponse("la réponse ne contient aucun choix".to_string()))?;

        ChatMessage::try_from(choice.message)
    }
}

impl Role {
    fn as_wire_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }

    fn from_wire_str(value: &str) -> Result<Self, ModelError> {
        match value {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            other => Err(ModelError::InvalidResponse(format!("rôle de message inconnu : « {other} »"))),
        }
    }
}

#[derive(Serialize)]
struct WireRequest<'req> {
    model: &'req str,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireToolDefinition>,
}

#[derive(Serialize, Deserialize, Default)]
struct WireMessage {
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct WireToolCall {
    id: String,
    #[serde(rename = "type", default = "wire_tool_call_type")]
    kind: String,
    function: WireFunctionCall,
}

fn wire_tool_call_type() -> String {
    "function".to_string()
}

#[derive(Serialize, Deserialize)]
struct WireFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct WireToolDefinition {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunctionDefinition,
}

#[derive(Serialize)]
struct WireFunctionDefinition {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Deserialize)]
struct WireCompletionResponse {
    choices: Vec<WireChoice>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireMessage,
}

impl From<&ChatMessage> for WireMessage {
    fn from(message: &ChatMessage) -> Self {
        Self {
            role: message.role.as_wire_str().to_string(),
            content: message.content.clone(),
            tool_calls: message.tool_calls.iter().map(WireToolCall::from).collect(),
            tool_call_id: message.tool_call_id.clone(),
        }
    }
}

impl From<&ToolCall> for WireToolCall {
    fn from(call: &ToolCall) -> Self {
        Self {
            id: call.id.clone(),
            kind: "function".to_string(),
            function: WireFunctionCall { name: call.name.clone(), arguments: call.arguments.to_string() },
        }
    }
}

impl From<&ToolDefinition> for WireToolDefinition {
    fn from(definition: &ToolDefinition) -> Self {
        Self {
            kind: "function",
            function: WireFunctionDefinition {
                name: definition.name.clone(),
                description: definition.description.clone(),
                parameters: definition.parameters.clone(),
            },
        }
    }
}

impl TryFrom<WireMessage> for ChatMessage {
    type Error = ModelError;

    fn try_from(message: WireMessage) -> Result<Self, Self::Error> {
        let tool_calls = message
            .tool_calls
            .into_iter()
            .map(ToolCall::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            role: Role::from_wire_str(&message.role)?,
            content: message.content,
            tool_calls,
            tool_call_id: message.tool_call_id,
        })
    }
}

impl TryFrom<WireToolCall> for ToolCall {
    type Error = ModelError;

    fn try_from(call: WireToolCall) -> Result<Self, Self::Error> {
        let arguments = serde_json::from_str(&call.function.arguments).map_err(|error| {
            ModelError::InvalidResponse(format!(
                "arguments d'appel d'outil invalides pour « {} » : {error}",
                call.function.name
            ))
        })?;

        Ok(Self { id: call.id, name: call.function.name, arguments })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_message_round_trip_preserves_tool_calls() {
        let original = ChatMessage::assistant_tool_calls(vec![ToolCall {
            id: "call_1".to_string(),
            name: "icpe_query".to_string(),
            arguments: serde_json::json!({ "code_postal": "33240" }),
        }]);

        let wire = WireMessage::from(&original);
        let round_tripped = ChatMessage::try_from(wire).expect("conversion valide");

        assert_eq!(original, round_tripped);
    }

    #[test]
    fn tool_result_message_carries_its_call_id() {
        let message = ChatMessage::tool_result("call_1", "ok");
        assert_eq!(message.role, Role::Tool);
        assert_eq!(message.tool_call_id, Some("call_1".to_string()));
    }
}
