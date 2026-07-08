//! Agent IA opérant par orchestration hiérarchique : un Superviseur délègue
//! dynamiquement des sous-tâches à des agents experts éphémères (voir
//! [`orchestration`]), chacun disposant d'un sous-ensemble d'outils
//! (`legifrance_search`, `fill_section`...) tiré d'un catalogue de profils
//! (voir [`catalog`]) plutôt que codé en dur, pour rédiger tout ou partie
//! d'un arrêté préfectoral.
//!
//! Le choix du modèle de langage est entièrement découplé de l'orchestration
//! : [`LanguageModel`] est une abstraction que n'importe quel fournisseur
//! compatible avec l'API de complétion de chat OpenAI peut implémenter
//! ([`OpenAiCompatibleModel`] le fait pour tous), ce qui permet de changer de
//! modèle (cloud ou auto-hébergé) par simple configuration.
//!
//! Ce crate ne dépend d'aucun type du domaine (`app`, `content`) : les
//! outils qui doivent agir sur l'état réel d'un projet (métadonnées,
//! structure de l'acte...) le font via les ports définis dans [`ports`], et
//! le catalogue d'experts via [`catalog::AgentCatalog`], que l'application
//! hôte implémente. [`orchestration::OrchestrationRun`] est entièrement
//! sérialisable : c'est à l'application hôte de le persister pour qu'une
//! pause (question à l'utilisateur, confirmation...) survive à une
//! déconnexion ou un redémarrage — ce crate ne bloque jamais lui-même en
//! attendant une réponse humaine.
//!
//! L'orchestration, le modèle et les outils ne sont disponibles que sous la
//! feature `server` (activée par défaut) : ils dépendent de
//! `reqwest`/`tokio`, indisponibles en WASM. Le composant Leptos
//! [`AgentPanel`] n'en dépend pas et reste donc utilisable côté client
//! (feature `hydrate`).

#[cfg(feature = "server")]
pub mod catalog;
#[cfg(feature = "server")]
mod error;
#[cfg(feature = "server")]
mod model;
#[cfg(feature = "server")]
mod observer;
#[cfg(feature = "server")]
pub mod orchestration;
pub mod panel;
#[cfg(feature = "server")]
pub mod ports;
#[cfg(feature = "server")]
mod tool;
#[cfg(feature = "server")]
pub mod tools;

#[cfg(feature = "server")]
pub use catalog::{AgentCatalog, AgentProfile};
#[cfg(feature = "server")]
pub use error::{AgentError, ModelError, ToolError};
#[cfg(feature = "server")]
pub use model::{
    ChatMessage, LanguageModel, OpenAiCompatibleModel, OpenAiCompatibleModelConfig, Role,
    StreamEvent, ToolCall, ToolDefinition,
};
#[cfg(feature = "server")]
pub use observer::{AgentObserver, NullAgentObserver};
#[cfg(feature = "server")]
pub use orchestration::{
    AgentFrame, OrchestrationRun, Orchestrator, PauseAnswer, PauseReason, PendingTurn, RunOutcome,
    RunStatus,
};
pub use panel::{
    AgentPanel, DocumentRequest, DocumentUpload, InteractionRequest, InteractionResponse,
    PanelEntry, PanelMessage, PanelQuestion, PanelQuestionAnswer, PanelReasoning, PanelRole,
    PanelToolCall, PanelToolCallStatus,
};
#[cfg(feature = "server")]
pub use tool::{DelegateRequest, PauseRequest, Tool, ToolOutput, ToolRegistry};
