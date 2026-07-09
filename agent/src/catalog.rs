//! Catalogue des profils d'agents experts que l'Orchestrateur peut
//! instancier à la volée (voir [`crate::orchestration::Orchestrator`]) :
//! au lieu d'une struct Rust dédiée par expert (Visas, Motifs...), chaque
//! expert n'est qu'une donnée — un [`AgentProfile`] — résolue par son
//! identifiant technique au moment de la délégation. L'application hôte
//! fournit le catalogue effectif (typiquement adossé à une table éditable
//! depuis un écran d'administration) via [`AgentCatalog`].

use async_trait::async_trait;

use crate::error::ToolError;

/// Configuration d'un agent expert éphémère : tout ce dont l'Orchestrateur a
/// besoin pour instancier un [`crate::orchestration::AgentFrame`] à la volée
/// quand il délègue une tâche à cet expert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    /// Identifiant technique stable, utilisé comme valeur du paramètre
    /// `expert_id` de l'outil `delegate_to_expert` (voir
    /// `agent::tools::DelegateToExpertTool`) — jamais affiché tel quel.
    pub id: String,
    /// Libellé affiché dans le panneau de conversation (voir
    /// `agent::panel`) et dans la description de l'outil de délégation.
    pub display_name: String,
    pub system_prompt: String,
    /// Sous-ensemble du registre complet d'outils auquel cet expert a accès
    /// (voir [`crate::tool::ToolRegistry::subset`]).
    pub tool_names: Vec<String>,
    pub max_steps: u32,
    /// Identifiant opaque (résolu par l'application hôte, voir
    /// [`crate::orchestration::Orchestrator::new`]) du modèle de langage
    /// dédié à cet expert. `None` fait exécuter cet expert par le modèle par
    /// défaut de l'Orchestrateur, comme avant l'introduction de ce champ —
    /// utile pour confier certaines tâches à un modèle plus adapté (plus
    /// rigoureux, plus rapide, moins coûteux...) que le modèle par défaut.
    pub model_id: Option<String>,
}

/// Point d'intégration vers le catalogue de profils d'experts disponibles.
/// L'implémentation concrète (typiquement adossée à une table administrable)
/// est fournie par l'application hôte, à la manière des autres ports de ce
/// crate (voir [`crate::ports`]).
#[async_trait]
pub trait AgentCatalog: Send + Sync {
    /// Liste les profils disponibles, pour construire le schéma JSON
    /// (`expert_id` en énumération) de l'outil `delegate_to_expert`.
    async fn list(&self) -> Result<Vec<AgentProfile>, ToolError>;

    /// Résout un profil par son identifiant technique, au moment où
    /// l'Orchestrateur exécute une délégation. `Ok(None)` si aucun profil
    /// correspondant n'existe (ex. modifié/supprimé du catalogue depuis que
    /// le modèle a vu la liste).
    async fn get(&self, id: &str) -> Result<Option<AgentProfile>, ToolError>;
}
