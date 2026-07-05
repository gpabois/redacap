use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Portée de disponibilité d'un outil de l'agent IA pour un domaine donné
/// (voir `agent::tools::CONFIGURABLE_TOOLS` pour le catalogue des outils
/// concernés). `domain_id: None` signifie une disponibilité globale (ex.
/// Légifrance) ; `domain_id: Some(_)` réserve l'outil à ce domaine précis
/// (ex. GéoRisques pour le domaine « Installation classée »).
///
/// Contrairement aux autres modèles de ce module, une portée n'a pas
/// d'identifiant propre : la paire `(tool_name, domain_id)` en tient lieu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolScope {
    pub tool_name: String,
    pub domain_id: Option<ID>,
}
