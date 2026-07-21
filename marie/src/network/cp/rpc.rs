use libp2p::request_response;
use serde::{Deserialize, Serialize};

use crate::{
    job::{Job, JobId, JobState},
    model::declaration::{ModelDeclaration, ModelId},
    session::SessionId,
    tools::{catalog::ToolId, declaration::ToolDeclaration},
};

/// Represents a Rpc Call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcCall {
    pub name: String,
    pub args: serde_json::Value
}

impl RpcCall {
    pub const GET_MODEL: &str = "get-model";
    /// Client -> control plane : crée ou remplace la déclaration d'un modèle
    /// (répliqué via Raft, voir `ControlPlaneRequest::SetModel`). Les
    /// arguments sont un [`SetModelRequest`].
    pub const SET_MODEL: &str = "set-model";
    /// Client -> control plane : retire un modèle du catalogue (répliqué via
    /// Raft, voir `ControlPlaneRequest::RemoveModel`). Les arguments sont un
    /// [`ModelId`].
    pub const REMOVE_MODEL: &str = "remove-model";
    /// Client -> control plane : liste tout le catalogue. Comme
    /// `GET_MODEL`, chaque clé API est chiffrée spécifiquement pour le nœud
    /// appelant (voir `SecretManager::encrypt_api_key`) — jamais en clair.
    pub const LIST_MODELS: &str = "list-models";
    /// Client -> control plane : crée ou remplace la déclaration d'un tool
    /// (répliqué via Raft, voir `ControlPlaneRequest::SetTool`). Les
    /// arguments sont un [`SetToolRequest`]. Ne dit rien de qui exécute ce
    /// tool — voir `RpcCall::REGISTER_RPC` et
    /// `tools::client::ToolClient::register_executor`.
    pub const SET_TOOL: &str = "set-tool";
    /// Client -> control plane : retire un tool du catalogue (répliqué via
    /// Raft, voir `ControlPlaneRequest::RemoveTool`). Les arguments sont un
    /// [`ToolId`].
    pub const REMOVE_TOOL: &str = "remove-tool";
    /// Client -> control plane : récupère la déclaration d'un tool. Les
    /// arguments sont un [`ToolId`].
    pub const GET_TOOL: &str = "get-tool";
    /// Client -> control plane : liste tout le catalogue de tools.
    pub const LIST_TOOLS: &str = "list-tools";
    pub const APPEND_ENTRIES: &str = "append-entries";
    pub const INSTALL_SNAPSHOT: &str = "install-snapshot";
    pub const VOTE: &str = "vote";
    /// Client -> control plane : propose un nouveau job (répliqué via Raft).
    pub const SUBMIT_JOB: &str = "submit-job";
    /// Control plane -> worker : demande d'exécuter le job joint. Best-effort :
    /// l'assignation fait foi dans l'état Raft, cet appel n'est qu'une notification.
    pub const RUN_JOB: &str = "run-job";
    /// Worker -> control plane : rapporte une transition d'état d'un job
    /// (répliquée via Raft).
    pub const REPORT_JOB_STATE: &str = "report-job-state";
    /// Vérificateur -> pair prétendant être `ControlPlane` : défi
    /// d'authentification (voir `secret::SecretManager::prove_membership`).
    /// Les arguments sont un nonce `[u8; 32]`, la réponse la preuve associée.
    pub const AUTH_CHALLENGE: &str = "auth-challenge";
    /// Pair -> control plane : s'enregistre comme exécuteur volontaire du nom
    /// de RPC donné en argument (`String`). Le control plane relaiera ensuite
    /// tout appel portant ce nom vers ce pair (voir `NetworkClient::register_rpc`).
    pub const REGISTER_RPC: &str = "register-rpc";
    /// Worker -> worker : demande le diff CRDT manquant d'une session (voir
    /// `session::crdt::YrsSession`) au pair qui la détient actuellement.
    /// Les arguments sont un [`SessionFetchRequest`], la réponse un diff
    /// `encode_diff_v1` prêt à être appliqué via `YrsSession::apply_diff`.
    pub const FETCH_SESSION: &str = "fetch-session";
    /// Client -> nœud de persistance : supprime définitivement une session
    /// (voir `persistency::SessionStore`) et tous ses fichiers (voir
    /// `persistency::SessionFilesystem::delete_session`). Les arguments sont
    /// un [`SessionId`]. Irréversible : à n'appeler qu'une fois certain
    /// qu'aucun worker n'a plus besoin de cette session.
    pub const DELETE_SESSION: &str = "delete-session";
}

impl RpcCall {
    #[must_use]
    pub fn new(name: impl ToString, args: impl Serialize) -> Self {
        Self {
            name: name.to_string(),
            args: serde_json::to_value(args).unwrap()
        }
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub enum RpcResult {
    RpcOk(serde_json::Value),
    RpcErr(String)
}

/// Retour d'une RPC dont l'appelant ne se soucie que du succès/échec
/// transport (voir [`crate::network::actor::NetworkClient::rpc`]), pas du
/// contenu de la réponse : accepte n'importe quelle valeur JSON renvoyée par
/// la cible (`Value::Null`, ou un type de réponse concret ignoré, ex.
/// `ControlPlaneResponse`) sans chercher à la désérialiser en un type précis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Void;

impl Serialize for Void {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for Void {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Void)
    }
}

/// Rapport de transition d'état d'un job, échangé via [`RpcCall::REPORT_JOB_STATE`]
/// (worker -> control plane).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStateReport {
    pub job_id: JobId,
    pub state: JobState,
}

/// Charge utile de [`RpcCall::SET_MODEL`] (client -> control plane) : `id`
/// est distinct de la clé sous laquelle l'appelant range la déclaration
/// localement, mais c'est bien elle qui sert de clé dans le catalogue
/// répliqué (voir `ControlPlaneRequest::SetModel`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetModelRequest {
    pub id: ModelId,
    pub declaration: ModelDeclaration,
}

/// Charge utile de [`RpcCall::SET_TOOL`] (client -> control plane), sur le
/// même modèle que [`SetModelRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetToolRequest {
    pub id: ToolId,
    pub declaration: ToolDeclaration,
}

/// Requête de synchronisation d'une session, échangée via
/// [`RpcCall::FETCH_SESSION`] (worker -> worker). `state_vector` est le
/// vecteur d'état yrs (`StateVector::encode_v1`) du demandeur — vide s'il
/// n'a jamais vu cette session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFetchRequest {
    pub session_id: SessionId,
    pub state_vector: Vec<u8>,
}

/// Charge utile de [`RpcCall::RUN_JOB`] (control plane -> worker) : le job à
/// exécuter, accompagné des workers actuellement affectés à d'autres frames
/// de la même session (voir `ControlPlaneState::session_holders`), pour
/// permettre au worker de synchroniser son état CRDT via
/// [`RpcCall::FETCH_SESSION`] avant de reprendre l'exécution — vide si ce
/// worker est le premier à prendre en charge cette session (aucune
/// synchronisation nécessaire, elle est créée vierge). Une fois acquise, la
/// session reste synchronisée en continu entre tous ses détenteurs actifs
/// via gossip (voir `session_client::SessionClient`), donc un seul détenteur
/// connu suffit ici pour amorcer : les autres, s'il y en a, seront rattrapés
/// par ce flux.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunJobRequest {
    pub job: Job,
    pub known_holders: Vec<libp2p::PeerId>,
}

pub type Behaviour = request_response::json::Behaviour<RpcCall, RpcResult>;

