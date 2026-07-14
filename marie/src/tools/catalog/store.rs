use serde::{Deserialize, Serialize};

use crate::{
    persistency::store::Persisted,
    tools::{catalog::ToolId, declaration::ToolDeclaration},
};

/// Représentation persistée d'une entrée du catalogue de tools (voir
/// `network::cp::state::ControlPlaneStateMachineStore`), sur le même modèle
/// que `model::catalog::store::StoredModel` — sans chiffrement, une
/// déclaration de tool ne porte aucune information sensible (voir
/// [`ToolDeclaration`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTool {
    pub id: ToolId,
    pub declaration: ToolDeclaration,
}

impl Persisted for StoredTool {
    type Id = ToolId;

    const NAMESPACE: &'static str = "tool";

    fn encode(&self) -> Vec<u8> {
        // Uniquement des `String`/`Value` : la sérialisation JSON ne peut pas
        // échouer en pratique (même choix que `RpcCall::new`).
        serde_json::to_vec(self).unwrap()
    }

    fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}
