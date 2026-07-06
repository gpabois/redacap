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
    /// Efface l'historique de la conversation avec l'agent pour cette
    /// connexion : la prochaine `RunAgent` repart d'une conversation vide
    /// plutôt que de poursuivre celle en cours (voir
    /// `server::protocol::ClientMessage::ClearHistory`).
    ClearHistory,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// La tâche agent en cours s'est terminée avec succès. Le contenu de la
    /// réponse finale a déjà été relayé au fil de l'eau via
    /// `AgentContentDelta` : ce message ne fait que lever l'indicateur
    /// d'attente côté client.
    AgentDone,
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
    /// Fragment de réflexion (chaîne de raisonnement) du modèle pour le tour
    /// en cours. Absent des fournisseurs qui n'exposent pas de raisonnement.
    AgentReasoningDelta { delta: String },
    /// Fragment de réponse texte (narration ou réponse finale) du modèle
    /// pour le tour en cours.
    AgentContentDelta { delta: String },
    /// Le tour courant du modèle est terminé : les fragments de réflexion
    /// accumulés depuis le dernier `AgentStepFinished` peuvent être figés.
    AgentStepFinished,
    /// L'agent démarre l'appel de l'outil `name`, avant confirmation
    /// éventuelle et exécution.
    AgentToolCallStarted {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Le résultat de l'appel d'outil `id` est disponible : `ok` distingue
    /// un succès (`output` porte alors la sortie de l'outil) d'un échec
    /// (`output` porte alors le message d'erreur).
    AgentToolCallFinished {
        id: String,
        ok: bool,
        output: String,
    },
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
