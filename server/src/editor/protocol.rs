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
    /// Archive la session de conversation active avec l'agent pour cette
    /// salle (voir `storage::agent_session::archive_active_session_for_room`) :
    /// la prochaine [`ClientMessage::RunAgent`] ouvre une nouvelle session
    /// plutôt que de poursuivre celle en cours, qui reste consultable en
    /// lecture seule (voir [`ClientMessage::ListAgentSessions`]). Ignoré si
    /// un run est actuellement `running`/`paused` pour cette salle.
    ClearHistory,
    /// Demande la liste des sessions de conversation passées de
    /// l'utilisateur courant pour cette salle (voir
    /// `storage::agent_session::list_sessions_for_room`), la plus récente en
    /// premier — voir [`ServerMessage::AgentSessions`].
    ListAgentSessions,
    /// Demande la reconstruction en lecture seule du transcript d'une session
    /// passée (voir [`ServerMessage::AgentSessionHistory`]). N'affecte jamais
    /// la session active ni la conversation actuellement affichée : c'est une
    /// lecture pure, à afficher séparément par le client (voir
    /// `agent::AgentPanel`).
    GetAgentSessionHistory { session_id: String },
    /// Mise à jour Yrs (encodée base64) du document de commentaires/notes de
    /// travail (voir `legal_act::Review`), produite par une édition locale
    /// (nouveau commentaire, résolution, suppression...). Contrairement aux
    /// mises à jour du corps de l'acte, elle transite par ce protocole texte
    /// plutôt que par une trame binaire dédiée : le volume est bien plus
    /// faible et cela évite de dupliquer le multiplexage binaire de
    /// [`crate::ws`] pour un second document Yrs.
    ReviewUpdate { update: String },
    /// Demande le contexte brut (historique `agent::ChatMessage`, système
    /// compris) effectivement envoyé au modèle par le frame Superviseur du
    /// run le plus récent de cette salle (voir
    /// [`ServerMessage::SupervisorContext`]) : outil de diagnostic, distinct
    /// de [`Self::GetAgentSessionHistory`] qui n'en donne qu'une lecture
    /// simplifiée destinée à l'inspecteur.
    GetSupervisorContext,
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
    InteractionAsk {
        agent_label: String,
        question: String,
    },
    /// L'agent demande une confirmation oui/non avant une action irréversible.
    InteractionConfirm {
        agent_label: String,
        message: String,
    },
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
    /// Réponse à [`ClientMessage::ListAgentSessions`] : sessions passées
    /// (actives ou archivées) de l'utilisateur courant pour cette salle, les
    /// plus récentes en premier.
    AgentSessions { sessions: Vec<AgentSessionWire> },
    /// Réponse à [`ClientMessage::GetAgentSessionHistory`] : transcript
    /// reconstruit (lecture seule) d'une session passée.
    AgentSessionHistory {
        session_id: String,
        entries: Vec<AgentSessionEntryWire>,
    },
    /// Envoyé une fois à l'ouverture de la connexion si la salle a une
    /// session active dont le transcript n'est pas vide (voir
    /// `crate::editor::ws::restore_active_session`) : contrairement à
    /// [`Self::AgentSessionHistory`], ce transcript alimente directement la
    /// conversation affichée (`app::ws::RoomHandle::agent_messages`), pas un
    /// recouvrement en lecture seule — c'est ce qui permet de reprendre une
    /// conversation après un rechargement de page plutôt que de la retrouver
    /// vide.
    AgentActiveSession { entries: Vec<AgentSessionEntryWire> },
    /// Envoyé une fois à l'ouverture de la connexion si la salle a un run
    /// `"running"` (voir `crate::editor::ws::handle_socket`) : signale à une
    /// connexion qui rejoint après coup (nouvel onglet, reconnexion après un
    /// rechargement de page) qu'une tâche est toujours en cours, avant même
    /// que sa prochaine progression n'arrive sur `agent_events` — sans ce
    /// signal, l'inspecteur verrait la zone de saisie disponible alors qu'une
    /// tâche tourne déjà pour cette salle.
    AgentRunInProgress,
    /// Réponse à [`ClientMessage::GetSupervisorContext`] : contexte brut
    /// (système compris) de l'historique du frame Superviseur du run le plus
    /// récent de la salle, tel qu'envoyé au modèle. Vide si aucun run n'existe
    /// encore pour cette salle.
    SupervisorContext {
        entries: Vec<SupervisorContextEntryWire>,
    },
}

/// Résumé d'une session de conversation passée (voir
/// `shared::model::AgentSession`), tel qu'affiché dans la liste proposée par
/// [`ServerMessage::AgentSessions`].
#[derive(Debug, Serialize)]
pub struct AgentSessionWire {
    pub id: String,
    /// `"active" | "archived"` (voir `shared::model::AgentSession::status`).
    pub status: String,
    /// Horodatage RFC 3339 : le client se charge de sa mise en forme.
    pub created_at: String,
    pub archived_at: Option<String>,
    /// Aperçu (tronqué) du premier message envoyé dans cette session, pour
    /// la distinguer des autres dans la liste sans devoir en charger tout le
    /// transcript.
    pub preview: Option<String>,
}

/// Élément du transcript reconstruit d'une session passée (voir
/// [`ServerMessage::AgentSessionHistory`]), traduit depuis l'historique
/// `agent::ChatMessage` du frame Superviseur (voir
/// `crate::editor::ws::agent_session_history_from_chat_messages`). Volontairement
/// plus simple que [`AgentToolCallStarted`]/[`AgentToolCallFinished`] au fil
/// de l'eau : une session passée n'a plus besoin d'être affichée comme
/// "en cours d'exécution".
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentSessionEntryWire {
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolCall {
        name: String,
        arguments: serde_json::Value,
        output: String,
    },
}

/// Message brut du contexte du Superviseur (voir
/// [`ServerMessage::SupervisorContext`]), traduit depuis `agent::ChatMessage`
/// (voir `crate::editor::ws::supervisor_context_from_chat_messages`) :
/// contrairement à [`AgentSessionEntryWire`], le message système est conservé
/// et un appel d'outil n'est jamais fusionné avec son résultat, pour refléter
/// tel quel ce que le Superviseur envoie effectivement au modèle.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SupervisorContextEntryWire {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<SupervisorContextToolCallWire>,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// Appel d'outil porté par un message assistant du contexte brut (voir
/// [`SupervisorContextEntryWire::Assistant`]).
#[derive(Debug, Serialize)]
pub struct SupervisorContextToolCallWire {
    pub id: String,
    pub name: String,
    pub arguments: String,
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
