//! Messages JSON échangés sur le canal de contrôle du websocket, à côté
//! des trames binaires qui portent les mises à jour Yrs brutes (voir
//! [`crate::ws`]). Les mises à jour du document ne transitent jamais par
//! ce protocole : seuls le pilotage de la boucle agentique et les
//! interactions qu'elle déclenche le font.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Démarre la boucle agentique sur la salle courante avec la tâche donnée.
    RunAgent { task: String },
    /// Réponse à une [`ServerMessage::InteractionAsk`] /
    /// [`ServerMessage::InteractionConfirm`] / [`ServerMessage::InteractionQuestions`]
    /// précédente. La forme attendue de `value` dépend de la question posée
    /// (chaîne, booléen ou tableau de réponses).
    InteractionAnswer { value: serde_json::Value },
    /// Active ou désactive l'acceptation automatique des outils de l'agent
    /// qui demanderaient normalement une confirmation (`fill_section`,
    /// `insert_node`, `remove_node`...). Persiste pour cette connexion tant
    /// qu'il n'est pas désactivé explicitement, y compris entre plusieurs
    /// [`ClientMessage::RunAgent`].
    SetAutoAccept { enabled: bool },
    /// Signale le nœud actuellement ciblé par l'utilisateur dans l'éditeur
    /// (`Some`), ou l'absence de cible (`None`). L'agent peut ensuite viser
    /// ce nœud via le mot-clé `"selection"` dans `fill_section`/
    /// `insert_node`/`remove_node`, sans jamais avoir à connaître ni
    /// demander son identifiant technique (voir `crate::ports::WsLegalActEditor`).
    SetSelection { node_id: Option<String> },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Réponse finale de l'agent pour la tâche en cours.
    AgentDone { content: String },
    /// La boucle agentique a échoué (erreur de modèle, outil, etc.).
    AgentError { message: String },
    /// L'agent pose une question ouverte (outil `ask_user`).
    InteractionAsk { question: String },
    /// L'agent demande une confirmation oui/non avant une action irréversible.
    InteractionConfirm { message: String },
    /// L'agent présente un formulaire structuré (outil `ask_questions`).
    InteractionQuestions {
        prompt: String,
        questions: Vec<InteractionQuestionWire>,
    },
    /// Liste des utilisateurs actuellement connectés à la salle (voir
    /// `crate::editor::state::EditorRoom`), envoyée à ce client à la
    /// connexion puis à chaque changement (arrivée/départ d'un pair).
    Presence { users: Vec<PresenceUser> },
}

/// Identité d'un utilisateur connecté à la salle, telle qu'affichée par une
/// pastille de présence côté client (initiale + couleur déterministes, voir
/// `crate::editor::presence`).
#[derive(Debug, Clone, Serialize)]
pub struct PresenceUser {
    pub user_id: String,
    pub initial: String,
    pub color: String,
}

#[derive(Debug, Serialize)]
pub struct InteractionQuestionWire {
    pub id: String,
    pub label: String,
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct InteractionAnswerWire {
    pub question_id: String,
    pub value: String,
    #[serde(default)]
    pub unsatisfactory_reason: Option<String>,
}
