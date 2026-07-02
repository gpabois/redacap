use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::{
    error::{AgentError, ToolError},
    model::{ChatMessage, LanguageModel, ToolCall},
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
        Self { system_prompt: String::new(), max_steps: 16 }
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
        config: AgentConfig,
        auto_accept: Arc<AtomicBool>,
    ) -> Self {
        Self { model, tools, user_interaction, config, auto_accept }
    }

    /// Exécute la boucle agentique jusqu'à obtenir une réponse finale du
    /// modèle, ou jusqu'à atteindre `max_steps` itérations.
    pub async fn run(&self, task: &str) -> Result<String, AgentError> {
        let mut messages = Vec::new();
        if !self.config.system_prompt.is_empty() {
            messages.push(ChatMessage::system(self.config.system_prompt.clone()));
        }
        messages.push(ChatMessage::user(task));

        let tool_definitions = self.tools.definitions();

        for _ in 0..self.config.max_steps {
            let response = self.model.complete(&messages, &tool_definitions).await?;

            if response.tool_calls.is_empty() {
                return Ok(response.content.unwrap_or_default());
            }

            let tool_calls = response.tool_calls.clone();
            messages.push(response);

            for call in &tool_calls {
                let content = match self.dispatch_tool_call(call).await {
                    Ok(output) => output.0,
                    Err(error) => format!("erreur : {error}"),
                };
                messages.push(ChatMessage::tool_result(call.id.clone(), content));
            }
        }

        Err(AgentError::MaxStepsExceeded(self.config.max_steps))
    }

    async fn dispatch_tool_call(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        let Some(tool) = self.tools.get(&call.name) else {
            return Err(ToolError::Other(format!("outil inconnu : « {} »", call.name)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::ToolDefinition, tool::Tool};
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ScriptedModel {
        responses: std::sync::Mutex<Vec<ChatMessage>>,
    }

    #[async_trait]
    impl LanguageModel for ScriptedModel {
        fn model_name(&self) -> &str {
            "scripted"
        }

        async fn complete(
            &self,
            _messages: &[ChatMessage],
            _tools: &[ToolDefinition],
        ) -> Result<ChatMessage, crate::error::ModelError> {
            Ok(self.responses.lock().expect("verrou non empoisonné").remove(0))
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
                ChatMessage { role: crate::model::Role::Assistant, content: Some("terminé".to_string()), tool_calls: vec![], tool_call_id: None },
            ]),
        };

        let mut tools = ToolRegistry::new();
        tools.register(Box::new(CountingTool { calls: AtomicUsize::new(0), requires_confirmation: false }));

        let agent = Agent::new(
            Arc::new(model),
            tools,
            Arc::new(AlwaysConfirm),
            AgentConfig::default(),
            Arc::new(AtomicBool::new(false)),
        );

        let answer = agent.run("vérifie l'installation").await.expect("exécution réussie");
        assert_eq!(answer, "terminé");
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
                ChatMessage { role: crate::model::Role::Assistant, content: Some("terminé".to_string()), tool_calls: vec![], tool_call_id: None },
            ]),
        };

        let mut tools = ToolRegistry::new();
        tools.register(Box::new(CountingTool { calls: AtomicUsize::new(0), requires_confirmation: true }));

        let agent = Agent::new(
            Arc::new(model),
            tools,
            Arc::new(NeverConfirm),
            AgentConfig::default(),
            Arc::new(AtomicBool::new(true)),
        );

        let answer = agent.run("vérifie l'installation").await.expect("exécution réussie");
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
        tools.register(Box::new(CountingTool { calls: AtomicUsize::new(0), requires_confirmation: false }));

        let config = AgentConfig { max_steps: 2, ..AgentConfig::default() };
        let agent =
            Agent::new(Arc::new(model), tools, Arc::new(AlwaysConfirm), config, Arc::new(AtomicBool::new(false)));

        let result = agent.run("tâche").await;
        assert!(matches!(result, Err(AgentError::MaxStepsExceeded(2))));
    }
}
