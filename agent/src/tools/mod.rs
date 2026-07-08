//! Implémentations concrètes des outils listés dans la documentation de
//! l'agent IA : chaque outil est un [`crate::Tool`] indépendant, à
//! enregistrer dans un [`crate::ToolRegistry`].

mod delegate;
mod document;
mod georisques;
mod intention;
mod interaction;
mod legal_act_editor;
mod legifrance;
mod metadata;

pub use delegate::DelegateToExpertTool;
pub use document::ReadDocumentTool;
pub use georisques::{GeorisquesClient, GeorisquesConfig, GeorisquesQueryTool, IcpeQueryTool};
pub use intention::{AddIntentionTool, ListIntentionsTool, RemoveIntentionTool};
pub use interaction::{AskQuestionsTool, AskUserTool, RequestDocumentTool};
pub use legal_act_editor::{
    FillSectionTool, GenerateNumberingTool, InsertNodeTool, ReadStructureTool, ReadTitleTool,
    RemoveNodeTool, SetTitleTool, ValidateStructureTool,
};
pub use legifrance::{
    LegifranceClient, LegifranceConfig, LegifranceFetchTool, LegifranceSearchTool,
};
pub use metadata::{ReadMetadataTool, WriteMetadataTool};

/// Catalogue des outils dont la disponibilité est configurable par domaine
/// (voir `storage::agent_tool_scope`) : des outils d'accès à des API
/// externes, par opposition aux outils cœur d'édition/interaction (toujours
/// disponibles, non listés ici). Chaque paire est `(identifiant technique =
/// Tool::name(), libellé affiché dans le panneau administrateur)`.
pub const CONFIGURABLE_TOOLS: &[(&str, &str)] = &[
    ("legifrance_search", "Recherche Légifrance"),
    ("legifrance_fetch", "Lecture d'un texte Légifrance"),
    ("georisques_query", "Interrogation GéoRisques"),
    ("icpe_query", "Interrogation base ICPE"),
];
