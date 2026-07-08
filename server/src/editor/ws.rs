//! Handler websocket de collaboration : chaque connexion rejoint la
//! [`Room`] identifiée par `room_id` dans l'URL (`/ws/{room_id}`).
//!
//! Deux types de trames y transitent :
//! - des trames **binaires**, qui portent des mises à jour Yrs brutes
//!   (encodées via `encode_diff_v1`/`encode_state_as_update_v1`) : celles
//!   reçues d'un client sont appliquées au [`YrsBody`] partagé puis
//!   rediffusées à tous les autres pairs de la salle ;
//! - des trames **texte** JSON ([`ClientMessage`]/[`ServerMessage`]), qui
//!   pilotent l'orchestration hiérarchique (voir `agent::orchestration`) et
//!   relaient les interactions qu'elle déclenche (`ask_user`,
//!   `ask_questions`...), ainsi que les changements de présence
//!   ([`ServerMessage::Presence`]).
//!
//! Quand un outil de l'agent modifie le corps de l'acte (ex: `fill_section`),
//! la mise à jour Yrs qui en résulte est diffusée de la même façon qu'une
//! édition utilisateur : tous les clients convergent vers le même document.
//!
//! L'état d'une orchestration (voir [`agent::orchestration::OrchestrationRun`])
//! est persisté dans `agent_runs` (voir `storage::agent_run`) plutôt que
//! conservé en mémoire pour la durée de la connexion : une pause (question à
//! l'inspecteur, confirmation requise...) survit ainsi à une déconnexion ou
//! un redémarrage du serveur. Au plus un run `running`/`paused` existe par
//! salle (voir `agent_runs_active_per_room_idx`) ; une connexion qui rejoint
//! une salle dont le run est `paused` reçoit immédiatement la question en
//! attente (voir [`replay_pending_interaction`]), pour reprendre là où
//! l'inspecteur l'avait laissée, y compris depuis un tout autre onglet.
//!
//! La connexion n'est acceptée qu'après authentification par le cookie de
//! session (voir [`authenticate`]) : chaque mise à jour journalisée doit
//! pouvoir être attribuée à un auteur (`legal_act_updates.author_id`, `NOT
//! NULL`). La connexion rejoint puis quitte la salle via
//! [`super::state::EditorRooms`] (`get_or_create`/`release`), qui tient à
//! jour la liste des utilisateurs présents diffusée à chaque changement.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use agent::ports::{DocumentContentPort, LegalActEditorPort};
use agent::tools::{
    AddIntentionTool, AskQuestionsTool, AskUserTool, DelegateToExpertTool, FillSectionTool,
    GenerateNumberingTool, GeorisquesClient, GeorisquesConfig, GeorisquesQueryTool, IcpeQueryTool,
    InsertNodeTool, LegifranceClient, LegifranceConfig, LegifranceFetchTool, LegifranceSearchTool,
    ListIntentionsTool, ReadDocumentTool, ReadStructureTool, ReadTitleTool, RemoveIntentionTool,
    RemoveNodeTool, RequestDocumentTool, SetTitleTool, ValidateStructureTool,
};
use agent::{
    AgentCatalog, AgentFrame, AgentObserver, LanguageModel, OpenAiCompatibleModel,
    OpenAiCompatibleModelConfig, OrchestrationRun, Orchestrator, PauseAnswer, PauseReason,
    PauseRequest, RunOutcome, RunStatus, ToolRegistry,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::PrivateCookieJar;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use legal_act::BodyNodeId;
use shared::id::ID;
use tokio::sync::mpsc;
use yrs::updates::decoder::Decode;
use yrs::{ReadTxn, StateVector, Transact, Update};

use super::ports::{StorageAgentCatalog, WsIntentions, WsLegalActEditor, WsUserInteraction};
use super::presence::{color_for_id, display_initial};
use super::protocol::{ClientMessage, DocumentUploadWire, InteractionAnswerWire, InteractionQuestionWire, PresenceUser, ServerMessage};
use super::state::{EditorRoom, Presence};
use crate::auth::session::COOKIE_NAME;
use crate::state::{AppState, SessionKey};

const SUPERVISOR_SYSTEM_PROMPT: &str = "Tu es le superviseur d'une équipe d'agents experts qui \
    rédigent ensemble un arrêté préfectoral ICPE. Tu ne rédiges jamais toi-même le contenu de \
    l'acte : tu comprends la demande de l'inspecteur, tu consultes `read_structure`/`read_title` \
    pour connaître l'état actuel de l'acte, puis tu délègues chaque sous-tâche de rédaction à \
    l'expert approprié du catalogue via `delegate_to_expert`, en lui donnant une description \
    autonome et précise de ce qu'il doit faire (il ne voit pas cette conversation). Un expert \
    peut lui-même poser une question à l'inspecteur si une information lui manque : dans ce cas, \
    attends simplement sa réponse avant de reprendre. Pose toi-même une question à l'inspecteur \
    (`ask_user`/`ask_questions`) uniquement pour clarifier la demande globale ou trancher entre \
    plusieurs experts possibles, jamais pour une question de détail rédactionnel qui relève d'un \
    expert. Les intentions rédactionnelles du projet (ex. « mise en demeure », « sanction \
    administrative ») s'ajoutent ou se retirent uniquement sur demande explicite de l'inspecteur, \
    avec `add_intention`/`remove_intention` : appelle d'abord `list_intentions` pour connaître les \
    intentions disponibles pour le domaine du projet et leur identifiant, n'en invente jamais un. \
    Une fois toutes les délégations nécessaires terminées, résume en une phrase ce qui a été fait.";

/// Outils directement accessibles au Superviseur (voir
/// [`SUPERVISOR_SYSTEM_PROMPT`]) : lecture/orientation, interaction, gestion
/// des intentions et délégation — jamais les outils de rédaction eux-mêmes
/// (`fill_section`, `insert_node`...) ni les API externes, réservés aux
/// profils d'experts du catalogue (voir `storage::agent_profile`).
const SUPERVISOR_TOOL_NAMES: &[&str] = &[
    "read_structure",
    "read_title",
    "validate_structure",
    "read_document",
    "list_intentions",
    "add_intention",
    "remove_intention",
    "ask_user",
    "ask_questions",
    "request_document",
    "delegate_to_expert",
];

/// Nombre maximal de tours du Superviseur pour une tâche (chaque délégation
/// à un expert a son propre budget, voir `AgentProfile::max_steps`).
const SUPERVISOR_MAX_STEPS: u32 = 16;

/// Complète [`SUPERVISOR_SYSTEM_PROMPT`] avec le prompt système dédié du
/// modèle IA actif (voir `shared::model::AiModel::system_prompt`, ajouté en
/// entête), puis avec le contexte du domaine du projet et des intentions qui
/// lui sont associées, et résout l'ensemble des outils autorisés pour ce
/// domaine (voir `storage::agent_tool_scope::list_allowed_tool_names_for_domain`).
///
/// Renvoie le prompt de base (+ prompt du modèle) seul et un ensemble vide si
/// `legal_act_id` est absent ou si le projet, son domaine ou ses intentions ne
/// peuvent pas être chargés : une erreur ici ne doit jamais empêcher de
/// lancer l'orchestration, seulement priver le prompt de contexte
/// additionnel.
async fn build_agent_context(
    pool: &storage::Pool,
    legal_act_id: Option<ID>,
    ai_model_system_prompt: &str,
) -> (String, HashSet<String>) {
    let mut prompt = SUPERVISOR_SYSTEM_PROMPT.to_string();
    if !ai_model_system_prompt.trim().is_empty() {
        prompt.push_str(&format!("\n\n{ai_model_system_prompt}"));
    }

    let Some(legal_act_id) = legal_act_id else {
        return (prompt, HashSet::new());
    };
    let Ok(legal_act) = storage::legal_act::get_legal_act(pool, &legal_act_id).await else {
        return (prompt, HashSet::new());
    };

    if let Ok(domain) = storage::domain::get_domain(pool, &legal_act.domain_id).await
        && !domain.agent_context.trim().is_empty()
    {
        prompt.push_str(&format!(
            "\n\nContexte du domaine « {} » :\n{}",
            domain.name, domain.agent_context
        ));
    }

    if let Ok(intentions) =
        storage::intention::list_intentions_for_legal_act(pool, &legal_act_id).await
    {
        for intention in intentions {
            if !intention.agent_context.trim().is_empty() {
                prompt.push_str(&format!(
                    "\n\nIntention « {} » :\n{}",
                    intention.name, intention.agent_context
                ));
            }
        }
    }

    let allowed_tools =
        storage::agent_tool_scope::list_allowed_tool_names_for_domain(pool, &legal_act.domain_id)
            .await
            .map(|names| names.into_iter().collect())
            .unwrap_or_default();

    (prompt, allowed_tools)
}

pub async fn ws_handler(
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
    jar: PrivateCookieJar<SessionKey>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(presence) = authenticate(&state, &jar).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    ws.on_upgrade(move |socket| handle_socket(socket, room_id, presence, state))
        .into_response()
}

/// Authentifie la connexion via le cookie de session, et construit
/// l'identité de présence (initiale + couleur, voir `super::presence`)
/// diffusée aux autres pairs de la salle. `None` si la session est absente,
/// invalide ou expirée : la connexion websocket est alors refusée avant
/// même la mise à niveau (voir [`ws_handler`]).
async fn authenticate(state: &AppState, jar: &PrivateCookieJar<SessionKey>) -> Option<Presence> {
    let session_id: ID = jar.get(COOKIE_NAME)?.value().parse().ok()?;
    let session = storage::session::get_active_session(&state.store, &session_id)
        .await
        .ok()?;
    let user = storage::user::get_user(&state.store, &session.user_id)
        .await
        .ok();
    let initial = user.map_or_else(
        || "?".to_string(),
        |user| display_initial(&user.display_name),
    );
    Some(Presence {
        user_id: session.user_id,
        initial,
        color: color_for_id(&session.user_id),
    })
}

fn to_wire(users: &[Presence]) -> Vec<PresenceUser> {
    users
        .iter()
        .map(|user| PresenceUser {
            user_id: user.user_id.to_string(),
            initial: user.initial.clone(),
            color: user.color.clone(),
        })
        .collect()
}

async fn handle_socket(
    socket: WebSocket,
    room_id: String,
    presence: Presence,
    state: Arc<AppState>,
) {
    let author_id = presence.user_id;
    let (room, present_after_join) = state
        .rooms
        .get_or_create(&state.store, &room_id, presence)
        .await;
    // Diffuse la nouvelle présence aux pairs déjà connectés (qui sont, eux,
    // déjà abonnés à `room.presence` à ce stade) : la connexion courante
    // recevra son propre snapshot directement ci-dessous plutôt que par ce
    // canal, auquel elle ne s'abonne qu'après.
    let _ = room.presence.send(present_after_join.clone());

    let (mut sink, mut stream) = socket.split();

    let initial_update = {
        let body = room.body.lock().await;
        body.doc()
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    };
    if sink
        .send(Message::Binary(initial_update.into()))
        .await
        .is_err()
    {
        broadcast_departure(&room, &room_id, &state, &author_id);
        return;
    }
    let presence_message = ServerMessage::Presence {
        users: to_wire(&present_after_join),
    };
    if let Ok(text) = serde_json::to_string(&presence_message) {
        if sink.send(Message::Text(text.into())).await.is_err() {
            broadcast_departure(&room, &room_id, &state, &author_id);
            return;
        }
    }

    // État complet du document de commentaires/notes de travail (voir
    // `legal_act::Review`), pendant de la trame binaire `initial_update`
    // ci-dessus pour le corps de l'acte, mais relayé sur le canal texte
    // (voir `ClientMessage::ReviewUpdate`/`ServerMessage::ReviewUpdate`).
    let initial_review_update = {
        let review = room.review.lock().await;
        review
            .doc()
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    };
    let review_message = ServerMessage::ReviewUpdate {
        update: base64::engine::general_purpose::STANDARD.encode(initial_review_update),
    };
    if let Ok(text) = serde_json::to_string(&review_message) {
        if sink.send(Message::Text(text.into())).await.is_err() {
            broadcast_departure(&room, &room_id, &state, &author_id);
            return;
        }
    }

    let mut room_rx = room.updates.subscribe();
    let mut review_rx = room.review_updates.subscribe();
    let mut presence_rx = room.presence.subscribe();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();

    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                update = room_rx.recv() => match update {
                    Ok(bytes) => {
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                update = review_rx.recv() => match update {
                    Ok(bytes) => {
                        let message = ServerMessage::ReviewUpdate {
                            update: base64::engine::general_purpose::STANDARD.encode(bytes),
                        };
                        let Ok(text) = serde_json::to_string(&message) else { continue };
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                users = presence_rx.recv() => match users {
                    Ok(users) => {
                        let message = ServerMessage::Presence { users: to_wire(&users) };
                        let Ok(text) = serde_json::to_string(&message) else { continue };
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                message = out_rx.recv() => match message {
                    Some(message) => {
                        let Ok(text) = serde_json::to_string(&message) else { continue };
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }
    });

    // Nœud actuellement ciblé par l'utilisateur dans l'éditeur de cette
    // connexion (voir `ClientMessage::SetSelection` ci-dessous) : propre à
    // chaque connexion plutôt qu'à la `Room`, puisque deux inspecteurs
    // collaborant sur le même acte peuvent cibler des nœuds différents.
    let selection: Arc<StdMutex<Option<BodyNodeId>>> = Arc::new(StdMutex::new(None));
    let editor: Arc<dyn LegalActEditorPort> = Arc::new(WsLegalActEditor::new(
        room.clone(),
        selection.clone(),
        author_id,
    ));
    // Une seule instance concrète, utilisée à la fois comme port de lecture
    // de document et comme observateur de l'orchestration : les deux rôles
    // relaient vers le même client websocket (voir `WsUserInteraction` dans
    // `super::ports`).
    let ws_interaction = Arc::new(WsUserInteraction::new(out_tx.clone(), state.store.clone()));
    let document_content: Arc<dyn DocumentContentPort> = ws_interaction.clone();
    let agent_observer: Arc<dyn AgentObserver> = ws_interaction;
    // Propre à cette connexion : voir la note sur son pendant, `selection`,
    // ci-dessus. Une orchestration reprise depuis une autre connexion après
    // une déconnexion repart avec `auto_accept = false`, ce qui est sans
    // risque (au pire, une confirmation de plus est demandée).
    let auto_accept = Arc::new(AtomicBool::new(false));

    // Rejoue la question en attente si la salle a un run en pause (voir
    // module doc) : une connexion qui arrive (nouvel onglet, reconnexion
    // après coupure...) doit voir immédiatement où l'orchestration s'est
    // arrêtée, plutôt que de paraître silencieusement bloquée.
    if let Ok(Some(run)) = storage::agent_run::get_active_run_for_room(&state.store, &room_id).await
        && run.status == "paused"
        && let Some(message) = replay_pending_interaction(&run)
    {
        let _ = out_tx.send(message);
    }

    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Binary(bytes) => apply_and_broadcast(&room, &author_id, &bytes).await,
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::RunAgent { task }) => {
                    let already_active = storage::agent_run::get_active_run_for_room(&state.store, &room_id)
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                    if !already_active {
                        spawn_agent_run(
                            state.clone(),
                            editor.clone(),
                            document_content.clone(),
                            agent_observer.clone(),
                            out_tx.clone(),
                            auto_accept.clone(),
                            room_id.clone(),
                            room.legal_act_id(),
                            author_id,
                            AgentInput::Start { task },
                        );
                    }
                }
                Ok(ClientMessage::InteractionAnswer { value }) => {
                    let paused = storage::agent_run::get_active_run_for_room(&state.store, &room_id)
                        .await
                        .ok()
                        .flatten()
                        .is_some_and(|run| run.status == "paused");
                    if paused {
                        spawn_agent_run(
                            state.clone(),
                            editor.clone(),
                            document_content.clone(),
                            agent_observer.clone(),
                            out_tx.clone(),
                            auto_accept.clone(),
                            room_id.clone(),
                            room.legal_act_id(),
                            author_id,
                            AgentInput::Resume { value },
                        );
                    }
                }
                Ok(ClientMessage::SetAutoAccept { enabled }) => {
                    auto_accept.store(enabled, Ordering::SeqCst);
                }
                Ok(ClientMessage::SetSelection { node_id }) => {
                    let parsed = node_id
                        .as_deref()
                        .and_then(|raw| raw.parse::<BodyNodeId>().ok());
                    *selection.lock().expect("verrou non empoisonné") = parsed;
                }
                Ok(ClientMessage::ClearHistory) => {
                    let active = storage::agent_run::get_active_run_for_room(&state.store, &room_id)
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                    if !active {
                        let _ = storage::agent_run::clear_room_history(&state.store, &room_id).await;
                    }
                }
                Ok(ClientMessage::ReviewUpdate { update }) => {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(update) {
                        apply_and_broadcast_review(&room, &author_id, &bytes).await;
                    }
                }
                Err(_) => {}
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
    broadcast_departure(&room, &room_id, &state, &author_id);
}

/// Signale le départ de `author_id` de `room_id` : retire sa connexion du
/// registre de présence, et diffuse le nouveau snapshot aux pairs restants
/// (voir `super::state::EditorRooms::release`) — sans effet si c'était la
/// dernière connexion active (la salle est alors retirée du registre, il
/// n'y a plus personne à qui diffuser).
fn broadcast_departure(room: &Arc<EditorRoom>, room_id: &str, state: &AppState, author_id: &ID) {
    if let Some(remaining) = state.rooms.release(room_id, author_id) {
        let _ = room.presence.send(remaining);
    }
}

/// Applique une mise à jour Yrs reçue d'un client au document de la salle,
/// la journalise au nom de `author_id`, puis la rediffuse telle quelle aux
/// autres pairs (une mise à jour Yrs est valide indépendamment de l'état de
/// son destinataire).
async fn apply_and_broadcast(room: &Arc<EditorRoom>, author_id: &ID, bytes: &[u8]) {
    let Ok(update) = Update::decode_v1(bytes) else {
        return;
    };
    {
        let body = room.body.lock().await;
        if body.doc().transact_mut().apply_update(update).is_err() {
            return;
        }
    }
    room.record_and_broadcast(author_id, bytes.to_vec()).await;
}

/// Pendant de [`apply_and_broadcast`] pour le document de commentaires/notes
/// de travail (voir [`ClientMessage::ReviewUpdate`]).
async fn apply_and_broadcast_review(room: &Arc<EditorRoom>, author_id: &ID, bytes: &[u8]) {
    let Ok(update) = Update::decode_v1(bytes) else {
        return;
    };
    {
        let review = room.review.lock().await;
        if review.doc().transact_mut().apply_update(update).is_err() {
            return;
        }
    }
    room.record_and_broadcast_review(author_id, bytes.to_vec())
        .await;
}

/// Résout le modèle IA actif (voir `/admin/ai-models`,
/// `storage::ai_model::get_active_ai_model`) en un [`LanguageModel`] prêt à
/// l'emploi, en déchiffrant sa clé API avec `AppState::secret_encryption_key`.
///
/// Échoue avec un message destiné à l'utilisateur si aucun modèle n'est actif
/// ou si sa clé API ne peut pas être déchiffrée : contrairement au contexte de
/// domaine/intentions, l'absence de modèle empêche réellement de lancer
/// l'orchestration.
async fn build_active_language_model(
    pool: &storage::Pool,
    secret_key: Option<Vec<u8>>,
) -> Result<(Arc<dyn LanguageModel>, String), String> {
    let ai_model = storage::ai_model::get_active_ai_model(pool)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| {
            "aucun modèle IA actif n'est configuré (voir /admin/ai-models)".to_string()
        })?;
    let key = secret_key.ok_or_else(|| {
        "SECRET_ENCRYPTION_KEY absente : impossible de déchiffrer la clé API du modèle IA"
            .to_string()
    })?;
    let api_key = shared::crypto::decrypt(&key, &ai_model.api_key_encrypted)
        .map_err(|_| "échec du déchiffrement de la clé API du modèle IA".to_string())?;

    let model: Arc<dyn LanguageModel> =
        Arc::new(OpenAiCompatibleModel::new(OpenAiCompatibleModelConfig {
            base_url: ai_model.base_url,
            api_key,
            model: ai_model.model,
        }));
    Ok((model, ai_model.system_prompt))
}

/// Construit le client GéoRisques à partir de la configuration enregistrée
/// via `/admin/integrations` (voir `storage::external_credentials`). L'API
/// `v1` étant accessible sans jeton, l'absence ou l'échec de déchiffrement de
/// la clé ne fait jamais échouer la construction : le client fonctionne alors
/// sans jeton porteur (quota réduit).
async fn build_georisques_client(
    pool: &storage::Pool,
    secret_key: Option<Vec<u8>>,
) -> GeorisquesClient {
    let api_key = storage::external_credentials::get_georisques_credentials(pool)
        .await
        .ok()
        .flatten()
        .and_then(|credentials| credentials.api_key_encrypted)
        .zip(secret_key)
        .and_then(|(encrypted, key)| shared::crypto::decrypt(&key, &encrypted).ok());
    GeorisquesClient::new(GeorisquesConfig {
        api_key,
        ..GeorisquesConfig::default()
    })
}

/// Construit le client Légifrance à partir de la configuration enregistrée
/// via `/admin/integrations`. Renvoie `None` si `client_id`/`client_secret`
/// ne sont pas tous deux configurés et déchiffrables : les outils
/// `legifrance_search`/`legifrance_fetch` restent alors indisponibles plutôt
/// que d'empêcher le reste de l'orchestration de démarrer.
async fn build_legifrance_client(
    pool: &storage::Pool,
    secret_key: Option<Vec<u8>>,
) -> Option<LegifranceClient> {
    let credentials = storage::external_credentials::get_legifrance_credentials(pool)
        .await
        .ok()
        .flatten()?;
    let client_id = credentials.client_id?;
    let encrypted_secret = credentials.client_secret_encrypted?;
    let key = secret_key?;
    let client_secret = shared::crypto::decrypt(&key, &encrypted_secret).ok()?;
    Some(LegifranceClient::new(LegifranceConfig::new(
        client_id,
        client_secret,
    )))
}

/// Ce que déclenche une connexion websocket sur l'orchestration de sa salle :
/// démarrer une nouvelle tâche, ou répondre à l'interaction en attente d'un
/// run déjà en pause (voir [`spawn_agent_run`]).
enum AgentInput {
    Start { task: String },
    Resume { value: serde_json::Value },
}

/// Traduit une [`PauseRequest`] émise par l'orchestration en message à
/// envoyer au client (voir [`ServerMessage`]), en conservant le libellé du
/// frame qui l'a posée (Superviseur ou expert délégué).
fn pause_request_to_server_message(agent_label: String, request: PauseRequest) -> ServerMessage {
    match request {
        PauseRequest::Ask { question } => ServerMessage::InteractionAsk { agent_label, question },
        PauseRequest::Confirm { message } => ServerMessage::InteractionConfirm { agent_label, message },
        PauseRequest::AskQuestions { prompt, questions } => ServerMessage::InteractionQuestions {
            agent_label,
            prompt,
            questions: questions
                .into_iter()
                .map(|question| InteractionQuestionWire {
                    id: question.id,
                    label: question.label,
                    options: question.options,
                })
                .collect(),
        },
        PauseRequest::RequestDocument {
            prompt,
            accepted_mime_types,
        } => ServerMessage::InteractionRequestDocument {
            agent_label,
            prompt,
            accepted_mime_types,
        },
    }
}

/// Reconstruit, pour un run persisté en pause, le message à rejouer à une
/// connexion qui vient de rejoindre la salle (voir [`handle_socket`]).
/// `None` si `run.stack` ne peut pas être interprété (ne devrait pas
/// arriver : mieux vaut ne rien rejouer que de faire planter la connexion).
fn replay_pending_interaction(run: &shared::model::AgentRun) -> Option<ServerMessage> {
    let stack: Vec<AgentFrame> = serde_json::from_value(run.stack.clone()).ok()?;
    let frame = stack.last()?;
    let pending = frame.pending.as_ref()?;
    let PauseReason::Interaction(request) = &pending.reason else {
        return None;
    };
    Some(pause_request_to_server_message(
        frame.agent_label.clone(),
        request.clone(),
    ))
}

/// Convertit la réponse brute du client (`ClientMessage::InteractionAnswer`)
/// en [`PauseAnswer`] adaptée à `request`, en persistant au passage les
/// octets d'un document uploadé (voir `storage::agent_run::store_document`)
/// pour qu'il survive à la connexion courante.
async fn decode_pause_answer(
    pool: &storage::Pool,
    run_id: &ID,
    request: &PauseRequest,
    value: serde_json::Value,
) -> Result<PauseAnswer, String> {
    match request {
        PauseRequest::Ask { .. } => {
            let text: String = serde_json::from_value(value)
                .map_err(|error| format!("réponse invalide à la question : {error}"))?;
            Ok(PauseAnswer::Text(text))
        }
        PauseRequest::Confirm { .. } => {
            let confirmed: bool = serde_json::from_value(value)
                .map_err(|error| format!("réponse invalide à la confirmation : {error}"))?;
            Ok(PauseAnswer::Bool(confirmed))
        }
        PauseRequest::AskQuestions { .. } => {
            let answers: Vec<InteractionAnswerWire> = serde_json::from_value(value)
                .map_err(|error| format!("réponses invalides au formulaire : {error}"))?;
            Ok(PauseAnswer::Questions(
                answers
                    .into_iter()
                    .map(|answer| agent::ports::QuestionAnswer {
                        question_id: answer.question_id,
                        value: answer.value,
                        unsatisfactory_reason: answer.unsatisfactory_reason,
                    })
                    .collect(),
            ))
        }
        PauseRequest::RequestDocument { .. } => {
            let upload: DocumentUploadWire = serde_json::from_value(value).map_err(|error| {
                format!("réponse invalide à la demande de document : {error}")
            })?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&upload.content_base64)
                .map_err(|error| format!("contenu du document invalide (base64) : {error}"))?;
            let document = storage::agent_run::store_document(
                pool,
                run_id,
                &upload.file_name,
                &upload.mime_type,
                bytes,
            )
            .await
            .map_err(|error| error.to_string())?;
            Ok(PauseAnswer::Document(agent::ports::DocumentRef {
                id: document.id.to_string(),
                file_name: document.file_name,
                mime_type: document.mime_type,
            }))
        }
    }
}

/// Persiste l'état final de `run` (après [`Orchestrator::drive`]/
/// [`Orchestrator::resume`]) et renvoie le message correspondant à envoyer
/// au client. `drive_result` porte soit l'issue de l'orchestration, soit
/// l'erreur qui l'a arrêtée — auquel cas `run.status` est positionné à
/// [`RunStatus::Failed`] avant sauvegarde (voir la note sur ce point dans la
/// documentation d'[`Orchestrator::drive`]).
async fn persist_run_outcome(
    pool: &storage::Pool,
    run_id: &ID,
    version: i32,
    mut run: OrchestrationRun,
    drive_result: Result<RunOutcome, agent::AgentError>,
) -> ServerMessage {
    let message = match drive_result {
        Ok(RunOutcome::Done(_)) => ServerMessage::AgentDone,
        Ok(RunOutcome::Paused { agent_label, request }) => {
            pause_request_to_server_message(agent_label, request)
        }
        Err(error) => {
            run.status = RunStatus::Failed;
            ServerMessage::AgentError {
                message: error.to_string(),
            }
        }
    };

    let status = match run.status {
        RunStatus::Running => "running",
        RunStatus::Paused => "paused",
        RunStatus::Done => "done",
        RunStatus::Failed => "failed",
    };
    let Ok(stack) = serde_json::to_value(&run.stack) else {
        return ServerMessage::AgentError {
            message: "échec de sérialisation de l'orchestration".to_string(),
        };
    };

    match storage::agent_run::save_run(pool, run_id, version, status, stack, run.final_answer.as_deref())
        .await
    {
        Ok(_) => message,
        Err(error) => ServerMessage::AgentError {
            message: format!("échec de la sauvegarde de l'orchestration : {error}"),
        },
    }
}

fn spawn_agent_run(
    state: Arc<AppState>,
    editor: Arc<dyn LegalActEditorPort>,
    document_content: Arc<dyn DocumentContentPort>,
    agent_observer: Arc<dyn AgentObserver>,
    out_tx: mpsc::UnboundedSender<ServerMessage>,
    auto_accept: Arc<AtomicBool>,
    room_id: String,
    legal_act_id: Option<ID>,
    author_id: ID,
    input: AgentInput,
) {
    tokio::spawn(async move {
        let message = run_orchestration(
            &state,
            editor,
            document_content,
            agent_observer,
            auto_accept,
            &room_id,
            legal_act_id,
            author_id,
            input,
        )
        .await
        .unwrap_or_else(|message| ServerMessage::AgentError { message });
        let _ = out_tx.send(message);
    });
}

/// Construit l'orchestrateur (modèle, registre d'outils complet, catalogue
/// d'experts) puis démarre ou reprend un run selon `input`, et persiste son
/// issue (voir [`persist_run_outcome`]).
async fn run_orchestration(
    state: &Arc<AppState>,
    editor: Arc<dyn LegalActEditorPort>,
    document_content: Arc<dyn DocumentContentPort>,
    agent_observer: Arc<dyn AgentObserver>,
    auto_accept: Arc<AtomicBool>,
    room_id: &str,
    legal_act_id: Option<ID>,
    author_id: ID,
    input: AgentInput,
) -> Result<ServerMessage, String> {
    let secret_key = state.secret_encryption_key.clone();
    let (model, ai_model_system_prompt) =
        build_active_language_model(&state.store, secret_key.clone()).await?;
    let (system_prompt, allowed_tools) =
        build_agent_context(&state.store, legal_act_id, &ai_model_system_prompt).await;

    let mut tools = ToolRegistry::new();
    tools.register(Box::new(ReadStructureTool::new(editor.clone())));
    tools.register(Box::new(FillSectionTool::new(editor.clone())));
    tools.register(Box::new(InsertNodeTool::new(editor.clone())));
    tools.register(Box::new(RemoveNodeTool::new(editor.clone())));
    tools.register(Box::new(GenerateNumberingTool::new(editor.clone())));
    tools.register(Box::new(ValidateStructureTool::new(editor.clone())));
    tools.register(Box::new(ReadTitleTool::new(editor.clone())));
    tools.register(Box::new(SetTitleTool::new(editor)));
    tools.register(Box::new(AskUserTool));
    tools.register(Box::new(AskQuestionsTool));
    tools.register(Box::new(RequestDocumentTool));
    tools.register(Box::new(ReadDocumentTool::new(Some(document_content))));

    if let Some(legal_act_id) = legal_act_id {
        let intentions: Arc<dyn agent::ports::IntentionPort> = Arc::new(WsIntentions::new(
            state.store.clone(),
            legal_act_id,
            author_id,
        ));
        tools.register(Box::new(ListIntentionsTool::new(intentions.clone())));
        tools.register(Box::new(AddIntentionTool::new(intentions.clone())));
        tools.register(Box::new(RemoveIntentionTool::new(intentions)));
    }

    if allowed_tools.contains("georisques_query") || allowed_tools.contains("icpe_query") {
        let georisques_client =
            Arc::new(build_georisques_client(&state.store, secret_key.clone()).await);
        if allowed_tools.contains("georisques_query") {
            tools.register(Box::new(GeorisquesQueryTool::new(georisques_client.clone())));
        }
        if allowed_tools.contains("icpe_query") {
            tools.register(Box::new(IcpeQueryTool::new(georisques_client)));
        }
    }

    if allowed_tools.contains("legifrance_search") || allowed_tools.contains("legifrance_fetch") {
        if let Some(legifrance_client) =
            build_legifrance_client(&state.store, secret_key.clone()).await
        {
            let legifrance_client = Arc::new(legifrance_client);
            if allowed_tools.contains("legifrance_search") {
                tools.register(Box::new(LegifranceSearchTool::new(legifrance_client.clone())));
            }
            if allowed_tools.contains("legifrance_fetch") {
                tools.register(Box::new(LegifranceFetchTool::new(legifrance_client)));
            }
        }
    }

    let catalog = StorageAgentCatalog::new(state.store.clone());
    let profiles = catalog.list().await.map_err(|error| error.to_string())?;
    tools.register(Box::new(DelegateToExpertTool::new(&profiles)));

    let orchestrator = Orchestrator::new(model, tools, Arc::new(catalog), agent_observer, auto_accept);

    match input {
        AgentInput::Start { task } => {
            let root = match storage::agent_run::get_latest_run_for_room(&state.store, room_id)
                .await
                .ok()
                .flatten()
            {
                Some(previous) if previous.status == "done" => {
                    let mut stack: Vec<AgentFrame> =
                        serde_json::from_value(previous.stack).map_err(|error| error.to_string())?;
                    let root = stack.pop().ok_or_else(|| {
                        "run précédent terminé sans frame superviseur".to_string()
                    })?;
                    root.resume_as_new_task(&task)
                }
                _ => AgentFrame::supervisor(
                    system_prompt,
                    SUPERVISOR_TOOL_NAMES.iter().map(|name| name.to_string()).collect(),
                    SUPERVISOR_MAX_STEPS,
                    &task,
                ),
            };

            let mut run = OrchestrationRun::new(root);
            let initial_stack = serde_json::to_value(&run.stack).map_err(|error| error.to_string())?;
            let created = storage::agent_run::create_run(&state.store, room_id, &author_id, initial_stack)
                .await
                .map_err(|error| error.to_string())?;

            let drive_result = orchestrator.drive(&mut run).await;
            Ok(persist_run_outcome(&state.store, &created.id, created.version, run, drive_result).await)
        }
        AgentInput::Resume { value } => {
            let existing = storage::agent_run::get_active_run_for_room(&state.store, room_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "aucune tâche en attente sur cette salle".to_string())?;
            let stack: Vec<AgentFrame> =
                serde_json::from_value(existing.stack.clone()).map_err(|error| error.to_string())?;
            let pending_request = stack
                .last()
                .and_then(|frame| frame.pending.as_ref())
                .and_then(|pending| match &pending.reason {
                    PauseReason::Interaction(request) => Some(request.clone()),
                    PauseReason::Delegating => None,
                })
                .ok_or_else(|| "cette salle n'est pas en attente d'une réponse".to_string())?;

            let answer = decode_pause_answer(&state.store, &existing.id, &pending_request, value).await?;
            let mut run = OrchestrationRun {
                stack,
                status: RunStatus::Paused,
                final_answer: None,
            };
            let drive_result = orchestrator.resume(&mut run, answer).await;
            Ok(persist_run_outcome(&state.store, &existing.id, existing.version, run, drive_result).await)
        }
    }
}
