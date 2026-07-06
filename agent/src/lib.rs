//! Agent IA opérant par boucle agentique (ReAct), tel que décrit dans la
//! documentation du projet : il dispose d'un ensemble d'outils
//! (`legifrance_search`, `read_metadata`, `fill_section`...) qu'il compose
//! pour rédiger tout ou partie d'un arrêté préfectoral.
//!
//! Le choix du modèle de langage est entièrement découplé de la boucle
//! agentique : [`LanguageModel`] est une abstraction que n'importe quel
//! fournisseur compatible avec l'API de complétion de chat OpenAI peut
//! implémenter ([`OpenAiCompatibleModel`] le fait pour tous), ce qui permet
//! de changer de modèle (cloud ou auto-hébergé) par simple configuration.
//!
//! Ce crate ne dépend d'aucun type du domaine (`app`, `content`) : les
//! outils qui doivent agir sur l'état réel d'un projet (métadonnées,
//! structure de l'acte, interaction utilisateur) le font via les ports
//! définis dans [`ports`], que l'application hôte implémente.
//!
//! La boucle agentique, le modèle et les outils ne sont disponibles que
//! sous la feature `server` (activée par défaut) : ils dépendent de
//! `reqwest`/`tokio`, indisponibles en WASM. Le composant Leptos
//! [`AgentPanel`] n'en dépend pas et reste donc utilisable côté client
//! (feature `hydrate`).

#[cfg(feature = "server")]
mod error;
#[cfg(feature = "server")]
mod model;
#[cfg(feature = "server")]
mod observer;
pub mod panel;
#[cfg(feature = "server")]
pub mod ports;
#[cfg(feature = "server")]
mod react;
#[cfg(feature = "server")]
mod tool;
#[cfg(feature = "server")]
pub mod tools;

#[cfg(feature = "server")]
pub use error::{AgentError, ModelError, ToolError};
#[cfg(feature = "server")]
pub use model::{
    ChatMessage, LanguageModel, OpenAiCompatibleModel, OpenAiCompatibleModelConfig, Role,
    StreamEvent, ToolCall, ToolDefinition,
};
#[cfg(feature = "server")]
pub use observer::{AgentObserver, NullAgentObserver};
pub use panel::{
    AgentPanel, InteractionRequest, InteractionResponse, PanelEntry, PanelMessage, PanelQuestion,
    PanelQuestionAnswer, PanelReasoning, PanelRole, PanelToolCall, PanelToolCallStatus,
};
#[cfg(feature = "server")]
pub use react::{Agent, AgentConfig};
#[cfg(feature = "server")]
pub use tool::{Tool, ToolOutput, ToolRegistry};
