//! Point d'observation de la boucle agentique (voir
//! [`crate::orchestration::Orchestrator`]) : permet à l'application hôte de
//! tracer en direct les réflexions du modèle et les appels d'outils qu'il
//! déclenche, indépendamment du canal par lequel elle choisit de les relayer
//! (ex: websocket, voir `server::editor::ports::WsUserInteraction`, jusqu'à
//! [`crate::panel::AgentPanel`] côté client).

use async_trait::async_trait;

use crate::model::ToolCall;

/// Observateur notifié au fil de l'exécution de l'orchestration. Toutes les
/// méthodes ont un défaut sans effet : un appelant qui ne s'intéresse qu'à
/// certains événements n'a besoin d'implémenter que ceux-là (voir
/// [`NullAgentObserver`] pour le cas qui n'en trace aucun).
///
/// Chaque méthode reçoit `agent_label`, le libellé du frame à l'origine de
/// l'événement (`"Superviseur"`, ou `AgentProfile::display_name` d'un expert
/// éphémère) : dans une orchestration hiérarchique, plusieurs agents peuvent
/// produire des événements pour la même tâche, et l'application hôte doit
/// pouvoir les distinguer dans l'historique affiché.
#[async_trait]
pub trait AgentObserver: Send + Sync {
    /// Fragment de réflexion (chaîne de raisonnement) reçu du modèle.
    /// N'est jamais appelé pour les fournisseurs qui n'exposent pas de
    /// raisonnement dans leur réponse en flux.
    async fn on_reasoning_delta(&self, _agent_label: &str, _delta: &str) {}

    /// Fragment de réponse texte (narration ou réponse finale) reçu du modèle.
    async fn on_content_delta(&self, _agent_label: &str, _delta: &str) {}

    /// Le tour courant du modèle (réflexion + contenu de cette étape) est
    /// terminé : signale que la réflexion accumulée depuis le dernier appel
    /// peut être figée, avant que l'agent n'exécute d'éventuels appels
    /// d'outils ou n'entame le tour suivant.
    async fn on_turn_finished(&self, _agent_label: &str) {}

    /// L'agent démarre l'appel de l'outil `call`, avant confirmation
    /// éventuelle et exécution.
    async fn on_tool_call_started(&self, _agent_label: &str, _call: &ToolCall) {}

    /// Le résultat de l'appel d'outil `call_id` est disponible : `Ok` porte
    /// la sortie de l'outil, `Err` le message d'erreur (refus de
    /// confirmation compris).
    async fn on_tool_call_finished(
        &self,
        _agent_label: &str,
        _call_id: &str,
        _result: &Result<String, String>,
    ) {
    }
}

/// Observateur qui ignore tous les événements, pour les appelants qui ne
/// tracent pas l'exécution de la boucle agentique (ex: tests).
pub struct NullAgentObserver;

impl AgentObserver for NullAgentObserver {}
