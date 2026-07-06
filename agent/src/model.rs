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
use tokio::sync::mpsc;

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

/// Fragment produit au fil de l'eau par [`LanguageModel::stream`] : le
/// modèle répartit sa réponse entre réflexion (chaîne de raisonnement,
/// absente chez la plupart des fournisseurs mais renvoyée par certains sous
/// `reasoning_content`/`reasoning`), contenu textuel final, et appels
/// d'outils — ces derniers arrivant fragment par fragment (nom une fois,
/// arguments en plusieurs morceaux de JSON à concaténer) et distingués par
/// `index` lorsque le modèle en enchaîne plusieurs dans le même tour.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    ReasoningDelta(String),
    ContentDelta(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: Option<String>,
    },
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
        Self {
            role: Role::System,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Message de réponse contenant des appels d'outils à exécuter (pas de
    /// contenu textuel final).
    #[must_use]
    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: None,
            tool_calls,
            tool_call_id: None,
        }
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

/// Un fournisseur de modèle de langage capable de produire le prochain tour
/// d'une conversation, en tenant compte des outils disponibles.
#[async_trait]
pub trait LanguageModel: Send + Sync {
    /// Nom du modèle interrogé, à des fins de journalisation.
    fn model_name(&self) -> &str;

    /// Lance la complétion en flux : les fragments de réflexion, de contenu
    /// et d'appels d'outils sont livrés au fil de l'eau sur le canal
    /// renvoyé, à mesure que le fournisseur les produit — voir
    /// [`StreamEvent`]. Échoue immédiatement si la requête elle-même ne peut
    /// pas être établie (réseau, authentification...) ; une erreur survenant
    /// après coup, pendant la lecture du flux, est renvoyée comme dernier
    /// élément du canal plutôt que par cette méthode.
    async fn stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<mpsc::UnboundedReceiver<Result<StreamEvent, ModelError>>, ModelError>;
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
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }
}

#[async_trait]
impl LanguageModel for OpenAiCompatibleModel {
    fn model_name(&self) -> &str {
        &self.config.model
    }

    async fn stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<mpsc::UnboundedReceiver<Result<StreamEvent, ModelError>>, ModelError> {
        let wire_tools: Vec<WireToolDefinition> =
            tools.iter().map(WireToolDefinition::from).collect();
        let request = WireRequest {
            model: &self.config.model,
            messages: messages.iter().map(WireMessage::from).collect(),
            // Explicite plutôt qu'omis : certains points de terminaison
            // compatibles OpenAI (passerelles, proxys) traitent l'absence de
            // `tool_choice` comme `"none"` plutôt que le défaut standard
            // `"auto"`, ce qui fait répondre le modèle en texte libre sans
            // jamais appeler les outils pourtant transmis dans `tools`.
            tool_choice: (!wire_tools.is_empty()).then_some("auto"),
            tools: wire_tools,
            stream: true,
        };

        let mut response = self
            .http
            .post(format!(
                "{}/chat/completions",
                self.config.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut buffer = String::new();
            loop {
                let chunk = match response.chunk().await {
                    Ok(Some(bytes)) => bytes,
                    Ok(None) => break,
                    Err(error) => {
                        let _ = tx.send(Err(ModelError::from(error)));
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Les événements SSE sont séparés par une ligne vide ;
                // `buffer` peut contenir un événement incomplet en fin de
                // tampon si la frontière TCP tombe au milieu (on la laisse
                // alors pour le prochain morceau reçu).
                while let Some(pos) = buffer.find("\n\n") {
                    let block = buffer[..pos].to_string();
                    buffer.drain(..pos + 2);
                    match parse_sse_block(&block) {
                        Ok(SseBlock::Ignore) => {}
                        Ok(SseBlock::Done) => return,
                        Ok(SseBlock::Chunk(chunk)) => {
                            for event in events_from_chunk(chunk) {
                                if tx.send(Ok(event)).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(Err(error));
                            return;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

/// Résultat de l'analyse d'un bloc SSE (voir [`parse_sse_block`]).
enum SseBlock {
    /// Bloc sans ligne `data:` (commentaire, ping de tenue de connexion...).
    Ignore,
    /// Marqueur de fin de flux standard (`data: [DONE]`).
    Done,
    Chunk(WireStreamChunk),
}

/// Analyse un bloc SSE complet (les lignes entre deux séparateurs `\n\n`) :
/// concatène ses éventuelles lignes `data:` (un événement peut être
/// fragmenté sur plusieurs lignes `data:` successives selon la spécification
/// SSE) puis le désérialise comme fragment de complétion en flux.
fn parse_sse_block(block: &str) -> Result<SseBlock, ModelError> {
    let data = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");

    if data.is_empty() {
        return Ok(SseBlock::Ignore);
    }
    if data == "[DONE]" {
        return Ok(SseBlock::Done);
    }

    serde_json::from_str(&data)
        .map(SseBlock::Chunk)
        .map_err(|error| {
            ModelError::InvalidResponse(format!("fragment de flux invalide : {error}"))
        })
}

/// Traduit un fragment de complétion en flux en [`StreamEvent`]s : au plus
/// une réflexion et un contenu par choix (chacun ignoré s'il est vide, ce
/// qui est fréquent une fois le flux entamé), et un événement par appel
/// d'outil delta présent.
fn events_from_chunk(chunk: WireStreamChunk) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for choice in chunk.choices {
        let delta = choice.delta;
        if let Some(reasoning) = delta.reasoning_content
            && !reasoning.is_empty()
        {
            events.push(StreamEvent::ReasoningDelta(reasoning));
        }
        if let Some(content) = delta.content
            && !content.is_empty()
        {
            events.push(StreamEvent::ContentDelta(content));
        }
        for tool_call in delta.tool_calls {
            events.push(StreamEvent::ToolCallDelta {
                index: tool_call.index,
                id: tool_call.id,
                name: tool_call.function.as_ref().and_then(|f| f.name.clone()),
                arguments_delta: tool_call.function.and_then(|f| f.arguments),
            });
        }
    }
    events
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
            other => Err(ModelError::InvalidResponse(format!(
                "rôle de message inconnu : « {other} »"
            ))),
        }
    }
}

#[derive(Serialize)]
struct WireRequest<'req> {
    model: &'req str,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
    stream: bool,
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

/// Fragment de complétion en flux (`data: { ... }` d'un événement SSE),
/// format commun aux points de terminaison compatibles avec l'API de
/// complétion de chat OpenAI en mode `stream: true`.
#[derive(Deserialize)]
struct WireStreamChunk {
    #[serde(default)]
    choices: Vec<WireStreamChoice>,
}

#[derive(Deserialize, Default)]
struct WireStreamChoice {
    #[serde(default)]
    delta: WireStreamDelta,
}

#[derive(Deserialize, Default)]
struct WireStreamDelta {
    #[serde(default)]
    content: Option<String>,
    /// Chaîne de raisonnement, absente chez la plupart des fournisseurs :
    /// `reasoning_content` (DeepSeek et compatibles) et `reasoning`
    /// (OpenRouter) désignent le même concept sous deux noms différents
    /// selon le fournisseur, d'où l'alias plutôt qu'un second champ.
    #[serde(default, alias = "reasoning")]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireStreamToolCallDelta>,
}

#[derive(Deserialize)]
struct WireStreamToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<WireStreamFunctionDelta>,
}

#[derive(Deserialize)]
struct WireStreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
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
            function: WireFunctionCall {
                name: call.name.clone(),
                arguments: call.arguments.to_string(),
            },
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

        Ok(Self {
            id: call.id,
            name: call.function.name,
            arguments,
        })
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
