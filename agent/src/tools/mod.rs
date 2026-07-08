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

pub use delegate::{DelegateToExpertTool, SpawnExpertTool};
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

/// Catalogue complet des outils assignables à un profil d'expert (voir
/// `shared::model::AgentProfile::tool_names`), affiché par le panneau
/// administrateur `/admin/agent-profiles` : outils cœur d'édition/interaction
/// toujours disponibles, plus les outils configurables de
/// [`CONFIGURABLE_TOOLS`]. Chaque paire est `(identifiant technique =
/// Tool::name(), libellé affiché)`.
pub const AGENT_TOOL_CATALOG: &[(&str, &str)] = &[
    ("read_structure", "Lire la structure de l'acte"),
    ("read_title", "Lire le titre de l'acte"),
    ("set_title", "Modifier le titre de l'acte"),
    ("fill_section", "Remplir une section de l'acte"),
    ("insert_node", "Insérer un nœud dans l'acte"),
    ("remove_node", "Retirer un nœud de l'acte"),
    ("generate_numbering", "Recalculer la numérotation"),
    ("validate_structure", "Valider la structure de l'acte"),
    ("read_metadata", "Lire les métadonnées de l'acte"),
    ("write_metadata", "Modifier les métadonnées de l'acte"),
    ("read_document", "Lire un document fourni par l'utilisateur"),
    ("request_document", "Demander un document à l'utilisateur"),
    ("ask_user", "Poser une question à l'utilisateur"),
    ("ask_questions", "Poser plusieurs questions à l'utilisateur"),
    ("list_intentions", "Lister les intentions du projet"),
    ("add_intention", "Ajouter une intention"),
    ("remove_intention", "Retirer une intention"),
    ("legifrance_search", "Recherche Légifrance"),
    ("legifrance_fetch", "Lecture d'un texte Légifrance"),
    ("georisques_query", "Interrogation GéoRisques"),
    ("icpe_query", "Interrogation base ICPE"),
    (
        "spawn_expert",
        "Confier une sous-tâche à un nouveau Superviseur",
    ),
    (
        "delegate_to_expert",
        "Déléguer à un profil expert (réservé au Superviseur)",
    ),
];
