//! Messages JSON échangés sur le canal de contrôle du websocket, à côté
//! des trames binaires qui portent les mises à jour Yrs brutes (voir
//! [`crate::ws`]). Les mises à jour du document ne transitent jamais par
//! ce protocole : seuls le pilotage de l'orchestration et les interactions
//! qu'elle déclenche le font.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Démarre l'orchestration sur la salle courante avec la tâche donnée.
    /// Ignoré si un run est déjà `running`/`paused` pour cette salle (voir
    /// `storage::agent_run::get_active_run_for_room`).
    RunAgent { task: String },
    /// Réponse à une [`ServerMessage::InteractionAsk`] /
    /// [`ServerMessage::InteractionConfirm`] / [`ServerMessage::InteractionQuestions`] /
    /// [`ServerMessage::InteractionRequestDocument`] précédente, appliquée au
    /// run actuellement en pause pour cette salle (voir
    /// `storage::agent_run::get_active_run_for_room`) : la réponse peut donc
    /// être envoyée depuis une connexion différente de celle qui a formulé
    /// la demande, y compris après un redémarrage du serveur. La forme
    /// attendue de `value` dépend de la question posée (chaîne, booléen,
    /// tableau de réponses ou [`DocumentUploadWire`]).
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
    /// Efface l'historique de la conversation avec l'agent pour cette salle
    /// (voir `storage::agent_run`) : la prochaine [`ClientMessage::RunAgent`]
    /// repart d'une conversation vide plutôt que de poursuivre celle en
    /// cours. Ignoré si un run est actuellement `running`/`paused` pour
    /// cette salle.
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
    /// L'agent pose une question ouverte (outil `ask_user`). `agent_label`
    /// identifie le frame à l'origine de la question (`"Superviseur"` ou le
    /// libellé d'un expert délégué, voir `agent::orchestration::AgentFrame`).
    InteractionAsk { agent_label: String, question: String },
    /// L'agent demande une confirmation oui/non avant une action irréversible.
    InteractionConfirm { agent_label: String, message: String },
    /// L'agent présente un formulaire structuré (outil `ask_questions`).
    InteractionQuestions {
        agent_label: String,
        prompt: String,
        questions: Vec<InteractionQuestionWire>,
    },
    /// L'agent demande à l'utilisateur de fournir un document externe, upload
    /// (outil `request_document`). La réponse attendue via
    /// [`ClientMessage::InteractionAnswer`] est un [`DocumentUploadWire`].
    InteractionRequestDocument {
        agent_label: String,
        prompt: String,
        accepted_mime_types: Vec<String>,
    },
    /// Liste des utilisateurs actuellement connectés à la salle (voir
    /// `crate::editor::state::EditorRoom`), envoyée à ce client à la
    /// connexion puis à chaque changement (arrivée/départ d'un pair).
    Presence { users: Vec<PresenceUser> },
    /// Fragment de réflexion (chaîne de raisonnement) du modèle pour le tour
    /// en cours (voir `agent::AgentObserver::on_reasoning_delta`), émis par
    /// le frame `agent_label`. Absent des fournisseurs qui n'exposent pas de
    /// raisonnement.
    AgentReasoningDelta { agent_label: String, delta: String },
    /// Fragment de réponse texte (narration ou réponse finale) du modèle
    /// pour le tour en cours (voir `agent::AgentObserver::on_content_delta`),
    /// émis par le frame `agent_label`.
    AgentContentDelta { agent_label: String, delta: String },
    /// Le tour courant du modèle est terminé : les fragments de réflexion
    /// accumulés depuis le dernier `AgentStepFinished` peuvent être figés
    /// (voir `agent::AgentObserver::on_turn_finished`).
    AgentStepFinished,
    /// L'agent démarre l'appel de l'outil `name`, avant confirmation
    /// éventuelle et exécution (voir
    /// `agent::AgentObserver::on_tool_call_started`), pour le frame
    /// `agent_label`.
    AgentToolCallStarted {
        agent_label: String,
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Le résultat de l'appel d'outil `id` est disponible : `ok` distingue
    /// un succès (`output` porte alors la sortie de l'outil) d'un échec
    /// (`output` porte alors le message d'erreur, voir
    /// `agent::AgentObserver::on_tool_call_finished`).
    AgentToolCallFinished {
        agent_label: String,
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

/// Réponse à une [`ServerMessage::InteractionRequestDocument`], transmise
/// comme `value` d'un [`ClientMessage::InteractionAnswer`] : le contenu brut
/// du fichier choisi par l'utilisateur, encodé en base64 (voir
/// `super::ws::apply_interaction_answer`).
#[derive(Debug, Deserialize)]
pub struct DocumentUploadWire {
    pub file_name: String,
    pub mime_type: String,
    pub content_base64: String,
}
