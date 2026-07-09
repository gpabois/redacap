//! Miroir, côté client, du protocole JSON défini dans `server::protocol`
//! (voir `server/src/protocol.rs`). Les deux crates ne peuvent pas partager
//! ce module directement — `server` dépend déjà de `app` — ces types
//! restent donc synchronisés à la main avec ceux du serveur. Comme côté
//! serveur, les mises à jour du document (trames binaires Yrs) ne
//! transitent jamais par ce protocole texte : voir [`crate::ws`].

use serde::{Deserialize, Serialize};
use shared::broadcast::{DocumentsChangedEvent, MetadataChangedEvent};

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
    /// Archive la session de conversation active avec l'agent pour cette
    /// connexion : la prochaine `RunAgent` ouvre une nouvelle session plutôt
    /// que de poursuivre celle en cours, qui reste consultable en lecture
    /// seule (voir `server::editor::protocol::ClientMessage::ClearHistory`).
    ClearHistory,
    /// Demande la liste des sessions de conversation passées de
    /// l'utilisateur courant pour cette salle, la plus récente en premier
    /// (voir `server::editor::protocol::ClientMessage::ListAgentSessions`).
    ListAgentSessions,
    /// Demande la reconstruction en lecture seule du transcript d'une
    /// session passée, sans affecter la session active ni la conversation
    /// actuellement affichée (voir
    /// `server::editor::protocol::ClientMessage::GetAgentSessionHistory`).
    GetAgentSessionHistory { session_id: String },
    /// Mise à jour Yrs (base64) du document de commentaires/notes de
    /// travail, produite par une édition locale (voir
    /// `server::protocol::ClientMessage::ReviewUpdate`).
    ReviewUpdate { update: String },
    /// Demande le contexte brut (historique `agent::ChatMessage`, système
    /// compris) effectivement envoyé au modèle par le frame Superviseur du
    /// run le plus récent de cette salle (voir
    /// `server::editor::protocol::ClientMessage::GetSupervisorContext`).
    GetSupervisorContext,
    /// Demande l'arrêt immédiat du run agent en cours pour cette salle (voir
    /// `server::editor::protocol::ClientMessage::StopAgent`). Sans effet si
    /// aucun run n'est actuellement en cours.
    StopAgent,
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
    /// Le run agent en cours a été interrompu à la demande d'un utilisateur
    /// (voir [`ClientMessage::StopAgent`]) : distinct de [`Self::AgentError`],
    /// qui signale un échec plutôt qu'un arrêt volontaire.
    AgentStopped,
    /// L'agent pose une question ouverte (outil `ask_user`). `agent_label`
    /// identifie le frame à l'origine de la question (`"Superviseur"` ou le
    /// libellé d'un expert délégué).
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
    /// Liste des utilisateurs actuellement connectés à la salle, envoyée à
    /// la connexion puis à chaque changement (arrivée/départ d'un pair).
    Presence { users: Vec<PresenceUser> },
    /// Fragment de réflexion (chaîne de raisonnement) du modèle pour le tour
    /// en cours, émis par le frame `agent_label`. Absent des fournisseurs
    /// qui n'exposent pas de raisonnement.
    AgentReasoningDelta { agent_label: String, delta: String },
    /// Fragment de réponse texte (narration ou réponse finale) du modèle
    /// pour le tour en cours, émis par le frame `agent_label`.
    AgentContentDelta { agent_label: String, delta: String },
    /// Le tour courant du modèle est terminé : les fragments de réflexion
    /// accumulés depuis le dernier `AgentStepFinished` peuvent être figés.
    AgentStepFinished,
    /// L'agent démarre l'appel de l'outil `name`, avant confirmation
    /// éventuelle et exécution, pour le frame `agent_label`.
    AgentToolCallStarted {
        agent_label: String,
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Le résultat de l'appel d'outil `id` est disponible : `ok` distingue
    /// un succès (`output` porte alors la sortie de l'outil) d'un échec
    /// (`output` porte alors le message d'erreur).
    AgentToolCallFinished {
        agent_label: String,
        id: String,
        ok: bool,
        output: String,
    },
    /// Pendant de [`ClientMessage::ReviewUpdate`] : état complet (à la
    /// connexion) puis mises à jour incrémentales (base64) du document de
    /// commentaires/notes de travail.
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
    /// session active dont le transcript n'est pas vide : alimente
    /// directement `RoomHandle::agent_messages`, pour retrouver la
    /// conversation en cours après un rechargement de page plutôt que de la
    /// voir vide (voir `server::editor::protocol::ServerMessage::AgentActiveSession`).
    AgentActiveSession { entries: Vec<AgentSessionEntryWire> },
    /// Envoyé une fois à l'ouverture de la connexion si la salle a un run
    /// toujours en cours (voir `server::editor::ws::handle_socket`) : fait
    /// passer `RoomHandle::agent_pending` à `true` immédiatement, avant même
    /// que sa prochaine progression n'arrive, pour qu'un rechargement de page
    /// ou un nouvel onglet ne présente pas la zone de saisie comme
    /// disponible alors qu'une tâche tourne déjà pour cette salle.
    AgentRunInProgress,
    /// Réponse à [`ClientMessage::GetSupervisorContext`] : contexte brut
    /// (système compris) de l'historique du frame Superviseur du run le plus
    /// récent de la salle, tel qu'envoyé au modèle.
    SupervisorContext {
        entries: Vec<SupervisorContextEntryWire>,
    },
    /// Diffusé à chaque écriture ou suppression d'une métadonnée du projet,
    /// par l'agent ou par un autre utilisateur (voir
    /// `server::editor::protocol::ServerMessage::MetadataChanged`) : permet à
    /// `crate::pages::project_metadata::ProjectMetadataPanel` de se
    /// resynchroniser sans recharger la page (voir
    /// `RoomHandle::metadata_version`/`RoomHandle::last_metadata_change`).
    MetadataChanged(MetadataChangedEvent),
    /// Diffusé à chaque ajout ou suppression d'un document du projet, par
    /// l'agent ou par un autre utilisateur (voir
    /// `server::editor::protocol::ServerMessage::DocumentsChanged`) : permet
    /// à `crate::pages::project_documents::ProjectFilesPanel` de se
    /// resynchroniser sans recharger la page (voir
    /// `RoomHandle::files_version`/`RoomHandle::files_last_change`).
    DocumentsChanged(DocumentsChangedEvent),
}

/// Résumé d'une session de conversation passée (voir
/// `server::editor::protocol::AgentSessionWire`, son pendant côté serveur).
#[derive(Debug, Clone, Deserialize)]
pub struct AgentSessionWire {
    pub id: String,
    /// `"active" | "archived"`.
    pub status: String,
    /// Horodatage RFC 3339 : le client se charge de sa mise en forme.
    pub created_at: String,
    pub archived_at: Option<String>,
    pub preview: Option<String>,
}

/// Élément du transcript reconstruit d'une session passée (voir
/// `server::editor::protocol::AgentSessionEntryWire`, son pendant côté
/// serveur).
#[derive(Debug, Clone, Deserialize)]
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
/// `server::editor::protocol::SupervisorContextEntryWire`, son pendant côté
/// serveur) : contrairement à [`AgentSessionEntryWire`], le message système
/// est conservé et un appel d'outil n'est jamais fusionné avec son résultat.
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
pub struct SupervisorContextToolCallWire {
    pub id: String,
    pub name: String,
    pub arguments: String,
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

/// Réponse à une [`ServerMessage::InteractionRequestDocument`], transmise
/// comme `value` d'un [`ClientMessage::InteractionAnswer`] (voir
/// `server::protocol::DocumentUploadWire`, son pendant côté serveur).
#[derive(Debug, Serialize)]
pub struct DocumentUploadWire {
    pub file_name: String,
    pub mime_type: String,
    pub content_base64: String,
}
