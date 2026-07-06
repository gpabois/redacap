use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    error::{AgentError, ModelError, ToolError},
    model::{ChatMessage, LanguageModel, Role, StreamEvent, ToolCall},
    observer::AgentObserver,
    ports::UserInteractionPort,
    tool::{ToolOutput, ToolRegistry},
};

/// Paramètres de la boucle agentique.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Instructions système décrivant le rôle de l'agent et les règles à
    /// respecter (ex: ne jamais valider une action irréversible sans
    /// confirmation).
    pub system_prompt: String,
    /// Nombre maximal d'itérations d'appels d'outils avant abandon.
    pub max_steps: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            max_steps: 16,
        }
    }
}

/// Agent opérant par boucle agentique (ReAct) : à chaque itération, le
/// modèle de langage choisit soit de répondre, soit d'appeler un ou
/// plusieurs outils du [`ToolRegistry`] ; leurs résultats sont réinjectés
/// dans la conversation jusqu'à obtention d'une réponse finale.
pub struct Agent {
    model: Arc<dyn LanguageModel>,
    tools: ToolRegistry,
    user_interaction: Arc<dyn UserInteractionPort>,
    observer: Arc<dyn AgentObserver>,
    config: AgentConfig,
    /// Partagé avec l'appelant : quand `true`, les outils marqués
    /// [`crate::Tool::requires_confirmation`] s'exécutent sans passer par
    /// [`UserInteractionPort::confirm`] (option « accepter toutes les
    /// modifications » côté utilisateur). Lu à chaque appel d'outil plutôt
    /// qu'une seule fois à la construction, pour pouvoir être activé ou
    /// désactivé pendant l'exécution de la boucle.
    auto_accept: Arc<AtomicBool>,
}

impl Agent {
    #[must_use]
    pub fn new(
        model: Arc<dyn LanguageModel>,
        tools: ToolRegistry,
        user_interaction: Arc<dyn UserInteractionPort>,
        observer: Arc<dyn AgentObserver>,
        config: AgentConfig,
        auto_accept: Arc<AtomicBool>,
    ) -> Self {
        Self {
            model,
            tools,
            user_interaction,
            observer,
            config,
            auto_accept,
        }
    }

    /// Exécute la boucle agentique jusqu'à obtenir une réponse finale du
    /// modèle, ou jusqu'à atteindre `max_steps` itérations.
    ///
    /// `history` porte la conversation complète et doit être conservé par
    /// l'appelant d'un appel à l'autre : chaque nouvelle tâche est ajoutée à
    /// la suite des échanges précédents (messages utilisateur, réponses de
    /// l'assistant, résultats d'outils) plutôt que de repartir d'une
    /// conversation vide, sans quoi l'agent perd tout le contexte des tours
    /// précédents dès la tâche suivante.
    pub async fn run(
        &self,
        task: &str,
        history: &mut Vec<ChatMessage>,
    ) -> Result<String, AgentError> {
        if history.is_empty() && !self.config.system_prompt.is_empty() {
            history.push(ChatMessage::system(self.config.system_prompt.clone()));
        }
        history.push(ChatMessage::user(task));

        let tool_definitions = self.tools.definitions();

        for _ in 0..self.config.max_steps {
            let response = self.run_turn(history, &tool_definitions).await?;
            self.observer.on_turn_finished().await;

            if response.tool_calls.is_empty() {
                let content = response.content.clone().unwrap_or_default();
                history.push(response);
                return Ok(content);
            }

            let tool_calls = response.tool_calls.clone();
            history.push(response);

            for call in &tool_calls {
                self.observer.on_tool_call_started(call).await;
                let result = match self.dispatch_tool_call(call).await {
                    Ok(output) => Ok(output.0),
                    Err(error) => Err(error.to_string()),
                };
                self.observer.on_tool_call_finished(&call.id, &result).await;
                let content = result.unwrap_or_else(|error| format!("erreur : {error}"));
                history.push(ChatMessage::tool_result(call.id.clone(), content));
            }
        }

        Err(AgentError::MaxStepsExceeded(self.config.max_steps))
    }

    /// Consomme le flux d'un tour de modèle jusqu'à son terme, en notifiant
    /// [`Self::observer`] de chaque fragment de réflexion/contenu reçu, et
    /// accumule les fragments d'appels d'outils (identifiés par leur
    /// `index`, voir [`StreamEvent::ToolCallDelta`]) en [`ToolCall`]s
    /// complets une fois le flux terminé.
    async fn run_turn(
        &self,
        messages: &[ChatMessage],
        tool_definitions: &[crate::model::ToolDefinition],
    ) -> Result<ChatMessage, AgentError> {
        let mut events = self.model.stream(messages, tool_definitions).await?;

        let mut content = String::new();
        let mut tool_calls: BTreeMap<usize, PartialToolCall> = BTreeMap::new();

        while let Some(event) = events.recv().await {
            match event? {
                StreamEvent::ReasoningDelta(delta) => {
                    self.observer.on_reasoning_delta(&delta).await;
                }
                StreamEvent::ContentDelta(delta) => {
                    self.observer.on_content_delta(&delta).await;
                    content.push_str(&delta);
                }
                StreamEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_delta,
                } => {
                    let entry = tool_calls.entry(index).or_default();
                    if let Some(id) = id {
                        entry.id = id;
                    }
                    if let Some(name) = name {
                        entry.name = name;
                    }
                    if let Some(fragment) = arguments_delta {
                        entry.arguments.push_str(&fragment);
                    }
                }
            }
        }

        let tool_calls = tool_calls
            .into_values()
            .map(PartialToolCall::finish)
            .collect::<Result<Vec<_>, ModelError>>()?;

        Ok(ChatMessage {
            role: Role::Assistant,
            content: (!content.is_empty()).then_some(content),
            tool_calls,
            tool_call_id: None,
        })
    }

    async fn dispatch_tool_call(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        let Some(tool) = self.tools.get(&call.name) else {
            return Err(ToolError::Other(format!(
                "outil inconnu : « {} »",
                call.name
            )));
        };

        if tool.requires_confirmation() && !self.auto_accept.load(Ordering::Relaxed) {
            let confirmed = self
                .user_interaction
                .confirm(&format!(
                    "Autoriser l'outil « {} » avec les paramètres {} ?",
                    call.name, call.arguments
                ))
                .await?;

            if !confirmed {
                return Err(ToolError::Rejected);
            }
        }

        tool.call(call.arguments.clone()).await
    }
}

/// Appel d'outil en cours d'accumulation depuis les fragments successifs
/// d'un [`StreamEvent::ToolCallDelta`] partageant le même `index` (voir
/// [`Agent::run_turn`]) : `arguments` est la concaténation brute des
/// fragments de JSON reçus, analysée une fois le flux terminé par
/// [`Self::finish`].
#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl PartialToolCall {
    fn finish(self) -> Result<ToolCall, ModelError> {
        let arguments = serde_json::from_str(&self.arguments).map_err(|error| {
            ModelError::InvalidResponse(format!(
                "arguments d'appel d'outil invalides pour « {} » : {error}",
                self.name
            ))
        })?;
        Ok(ToolCall {
            id: self.id,
            name: self.name,
            arguments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::ToolDefinition, observer::NullAgentObserver, tool::Tool};
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    struct ScriptedModel {
        responses: std::sync::Mutex<Vec<ChatMessage>>,
    }

    #[async_trait]
    impl LanguageModel for ScriptedModel {
        fn model_name(&self) -> &str {
            "scripted"
        }

        async fn stream(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
        ) -> Result<mpsc::UnboundedReceiver<Result<StreamEvent, ModelError>>, ModelError> {
            let response = self
                .responses
                .lock()
                .expect("verrou non empoisonné")
                .remove(0);

            let (tx, rx) = mpsc::unbounded_channel();
            if let Some(content) = response.content {
                let _ = tx.send(Ok(StreamEvent::ContentDelta(content)));
            }
            for (index, call) in response.tool_calls.into_iter().enumerate() {
                let _ = tx.send(Ok(StreamEvent::ToolCallDelta {
                    index,
                    id: Some(call.id),
                    name: Some(call.name),
                    arguments_delta: Some(call.arguments.to_string()),
                }));
            }
            Ok(rx)
        }
    }

    struct CountingTool {
        calls: AtomicUsize,
        requires_confirmation: bool,
    }

    #[async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            "icpe_query"
        }

        fn description(&self) -> &str {
            "outil de test"
        }

        fn parameters_schema(&self) -> Value {
            json!({ "type": "object" })
        }

        fn requires_confirmation(&self) -> bool {
            self.requires_confirmation
        }

        async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolOutput::new("résultat"))
        }
    }

    struct AlwaysConfirm;

    #[async_trait]
    impl UserInteractionPort for AlwaysConfirm {
        async fn ask(&self, _question: &str) -> Result<String, ToolError> {
            Ok(String::new())
        }

        async fn confirm(&self, _message: &str) -> Result<bool, ToolError> {
            Ok(true)
        }

        async fn ask_questions(
            &self,
            _prompt: &str,
            _questions: &[crate::ports::Question],
        ) -> Result<Vec<crate::ports::QuestionAnswer>, ToolError> {
            Ok(Vec::new())
        }
    }

    struct NeverConfirm;

    #[async_trait]
    impl UserInteractionPort for NeverConfirm {
        async fn ask(&self, _question: &str) -> Result<String, ToolError> {
            Ok(String::new())
        }

        async fn confirm(&self, _message: &str) -> Result<bool, ToolError> {
            panic!("confirm ne doit pas être appelé quand l'auto-acceptation est active")
        }

        async fn ask_questions(
            &self,
            _prompt: &str,
            _questions: &[crate::ports::Question],
        ) -> Result<Vec<crate::ports::QuestionAnswer>, ToolError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn run_returns_final_answer_once_model_stops_calling_tools() {
        let model = ScriptedModel {
            responses: std::sync::Mutex::new(vec![
                ChatMessage::assistant_tool_calls(vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "icpe_query".to_string(),
                    arguments: json!({}),
                }]),
                ChatMessage {
                    role: crate::model::Role::Assistant,
                    content: Some("terminé".to_string()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ]),
        };

        let mut tools = ToolRegistry::new();
        tools.register(Box::new(CountingTool {
            calls: AtomicUsize::new(0),
            requires_confirmation: false,
        }));

        let agent = Agent::new(
            Arc::new(model),
            tools,
            Arc::new(AlwaysConfirm),
            Arc::new(NullAgentObserver),
            AgentConfig::default(),
            Arc::new(AtomicBool::new(false)),
        );

        let mut history = Vec::new();
        let answer = agent
            .run("vérifie l'installation", &mut history)
            .await
            .expect("exécution réussie");
        assert_eq!(answer, "terminé");
    }

    #[tokio::test]
    async fn run_reuses_history_across_successive_calls() {
        let model = ScriptedModel {
            responses: std::sync::Mutex::new(vec![
                ChatMessage {
                    role: crate::model::Role::Assistant,
                    content: Some("première réponse".to_string()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                ChatMessage {
                    role: crate::model::Role::Assistant,
                    content: Some("seconde réponse".to_string()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ]),
        };

        let agent = Agent::new(
            Arc::new(model),
            ToolRegistry::new(),
            Arc::new(AlwaysConfirm),
            Arc::new(NullAgentObserver),
            AgentConfig {
                system_prompt: "tu es Marie".to_string(),
                ..AgentConfig::default()
            },
            Arc::new(AtomicBool::new(false)),
        );

        let mut history = Vec::new();
        agent
            .run("premier message", &mut history)
            .await
            .expect("exécution réussie");
        agent
            .run("second message", &mut history)
            .await
            .expect("exécution réussie");

        // système + 1er message utilisateur + 1ère réponse + 2e message + 2e réponse
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].role, crate::model::Role::System);
        assert_eq!(history[1].content.as_deref(), Some("premier message"));
        assert_eq!(history[2].content.as_deref(), Some("première réponse"));
        assert_eq!(history[3].content.as_deref(), Some("second message"));
        assert_eq!(history[4].content.as_deref(), Some("seconde réponse"));
    }

    #[tokio::test]
    async fn auto_accept_bypasses_confirmation_for_tools_that_require_it() {
        let model = ScriptedModel {
            responses: std::sync::Mutex::new(vec![
                ChatMessage::assistant_tool_calls(vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "icpe_query".to_string(),
                    arguments: json!({}),
                }]),
                ChatMessage {
                    role: crate::model::Role::Assistant,
                    content: Some("terminé".to_string()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ]),
        };

        let mut tools = ToolRegistry::new();
        tools.register(Box::new(CountingTool {
            calls: AtomicUsize::new(0),
            requires_confirmation: true,
        }));

        let agent = Agent::new(
            Arc::new(model),
            tools,
            Arc::new(NeverConfirm),
            Arc::new(NullAgentObserver),
            AgentConfig::default(),
            Arc::new(AtomicBool::new(true)),
        );

        let mut history = Vec::new();
        let answer = agent
            .run("vérifie l'installation", &mut history)
            .await
            .expect("exécution réussie");
        assert_eq!(answer, "terminé");
    }

    #[tokio::test]
    async fn run_fails_after_max_steps_without_final_answer() {
        let infinite_tool_call = || {
            ChatMessage::assistant_tool_calls(vec![ToolCall {
                id: "call_1".to_string(),
                name: "icpe_query".to_string(),
                arguments: json!({}),
            }])
        };

        let model = ScriptedModel {
            responses: std::sync::Mutex::new(vec![infinite_tool_call(), infinite_tool_call()]),
        };

        let mut tools = ToolRegistry::new();
        tools.register(Box::new(CountingTool {
            calls: AtomicUsize::new(0),
            requires_confirmation: false,
        }));

        let config = AgentConfig {
            max_steps: 2,
            ..AgentConfig::default()
        };
        let agent = Agent::new(
            Arc::new(model),
            tools,
            Arc::new(AlwaysConfirm),
            Arc::new(NullAgentObserver),
            config,
            Arc::new(AtomicBool::new(false)),
        );

        let mut history = Vec::new();
        let result = agent.run("tâche", &mut history).await;
        assert!(matches!(result, Err(AgentError::MaxStepsExceeded(2))));
    }
}
