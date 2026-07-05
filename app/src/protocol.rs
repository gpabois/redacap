//! Miroir, côté client, du protocole JSON défini dans `server::protocol`
//! (voir `server/src/protocol.rs`). Les deux crates ne peuvent pas partager
//! ce module directement — `server` dépend déjà de `app` — ces types
//! restent donc synchronisés à la main avec ceux du serveur. Comme côté
//! serveur, les mises à jour du document (trames binaires Yrs) ne
//! transitent jamais par ce protocole texte : voir [`crate::ws`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Démarre la boucle agentique sur la salle courante avec la tâche donnée.
    RunAgent { task: String },
    /// Réponse à une [`ServerMessage::InteractionAsk`] /
    /// [`ServerMessage::InteractionConfirm`] / [`ServerMessage::InteractionQuestions`]
    /// précédente. La forme de `value` dépend de la question posée (chaîne,
    /// booléen ou tableau de réponses).
    InteractionAnswer { value: serde_json::Value },
    /// Active ou désactive l'acceptation automatique des outils de l'agent
    /// qui demanderaient normalement une confirmation.
    SetAutoAccept { enabled: bool },
    /// Signale le nœud actuellement ciblé par l'utilisateur dans l'éditeur
    /// (`Some`), ou l'absence de cible (`None`), pour que l'agent puisse le
    /// viser via le mot-clé `"selection"` sans jamais avoir à connaître son
    /// identifiant technique (voir `server::protocol::ClientMessage::SetSelection`).
    SetSelection { node_id: Option<String> },
}

#[derive(Debug, Deserialize)]
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
    /// Liste des utilisateurs actuellement connectés à la salle, envoyée à
    /// la connexion puis à chaque changement (arrivée/départ d'un pair).
    Presence { users: Vec<PresenceUser> },
}

/// Identité d'un utilisateur connecté à la salle (voir
/// `server::protocol::PresenceUser`, son pendant côté serveur) : initiale et
/// couleur déterministes, calculées par le serveur pour que tous les pairs
/// affichent la même pastille pour un même utilisateur.
#[derive(Debug, Clone, Deserialize)]
pub struct PresenceUser {
    pub user_id: String,
    pub initial: String,
    pub color: String,
}

#[derive(Debug, Deserialize)]
pub struct InteractionQuestionWire {
    pub id: String,
    pub label: String,
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct InteractionAnswerWire {
    pub question_id: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsatisfactory_reason: Option<String>,
}
