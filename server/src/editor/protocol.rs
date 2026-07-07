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
    /// Efface l'historique de la conversation avec l'agent pour cette
    /// connexion (voir `agent::Agent::run`'s `history`) : la prochaine
    /// [`ClientMessage::RunAgent`] repart d'une conversation vide plutôt que
    /// de poursuivre celle en cours. Ignoré si une tâche agent est en cours
    /// d'exécution sur cette connexion.
    ClearHistory,
    /// Mise à jour Yrs (encodée base64) du document de commentaires/notes de
    /// travail (voir `legal_act::Review`), produite par une édition locale
    /// (nouveau commentaire, résolution, suppression...). Contrairement aux
    /// mises à jour du corps de l'acte, elle transite par ce protocole texte
    /// plutôt que par une trame binaire dédiée : le volume est bien plus
    /// faible et cela évite de dupliquer le multiplexage binaire de
    /// [`crate::ws`] pour un second document Yrs.
    ReviewUpdate { update: String },
}

#[derive(Debug, Serialize)]
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
    /// Liste des utilisateurs actuellement connectés à la salle (voir
    /// `crate::editor::state::EditorRoom`), envoyée à ce client à la
    /// connexion puis à chaque changement (arrivée/départ d'un pair).
    Presence { users: Vec<PresenceUser> },
    /// Fragment de réflexion (chaîne de raisonnement) du modèle pour le tour
    /// en cours (voir `agent::AgentObserver::on_reasoning_delta`). Absent
    /// des fournisseurs qui n'exposent pas de raisonnement.
    AgentReasoningDelta { delta: String },
    /// Fragment de réponse texte (narration ou réponse finale) du modèle
    /// pour le tour en cours (voir `agent::AgentObserver::on_content_delta`).
    AgentContentDelta { delta: String },
    /// Le tour courant du modèle est terminé : les fragments de réflexion
    /// accumulés depuis le dernier `AgentStepFinished` peuvent être figés
    /// (voir `agent::AgentObserver::on_turn_finished`).
    AgentStepFinished,
    /// L'agent démarre l'appel de l'outil `name`, avant confirmation
    /// éventuelle et exécution (voir
    /// `agent::AgentObserver::on_tool_call_started`).
    AgentToolCallStarted {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Le résultat de l'appel d'outil `id` est disponible : `ok` distingue
    /// un succès (`output` porte alors la sortie de l'outil) d'un échec
    /// (`output` porte alors le message d'erreur, voir
    /// `agent::AgentObserver::on_tool_call_finished`).
    AgentToolCallFinished {
        id: String,
        ok: bool,
        output: String,
    },
    /// Pendant de [`ClientMessage::ReviewUpdate`] : mise à jour Yrs (base64)
    /// du document de commentaires/notes de travail, envoyée à la connexion
    /// à l'ouverture (état complet, voir `crate::ws::handle_socket`) puis à
    /// chaque changement (la sienne comme celles rediffusées des autres pairs).
    ReviewUpdate { update: String },
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
