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

use agent::ports::LegalActEditorPort;
use agent::tools::{
    AddIntentionTool, AskQuestionsTool, AskUserTool, DelegateToExpertTool, FetchDocumentByUrlTool,
    FillSectionTool, GenerateNumberingTool, GeorisquesClient, GeorisquesConfig,
    GeorisquesQueryTool, IcpeQueryTool, InsertNodeTool, LegifranceClient, LegifranceConfig,
    LegifranceFetchTool, LegifranceSearchTool, ListIntentionsTool, ReadDocumentTool,
    ReadMetadataTool, ReadStructureTool, ReadTitleTool, RemoveIntentionTool, RemoveNodeTool,
    RequestDocumentTool, SearchDocumentsTool, SearchMetadataTool, SetTitleTool, SpawnExpertTool,
    ValidateStructureTool, WriteMetadataTool,
};
use agent::{
    AgentCatalog, AgentFrame, AgentObserver, ChatMessage, LanguageModel, OpenAiCompatibleModel,
    OpenAiCompatibleModelConfig, OrchestrationRun, Orchestrator, PauseAnswer, PauseReason,
    PauseRequest, Role, RunOutcome, RunStatus, ToolRegistry,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::PrivateCookieJar;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use legal_act::NodeId;
use shared::broadcast::{DocumentChangeKind, DocumentsChangedEvent};
use shared::id::ID;
use tokio::sync::mpsc;
use yrs::updates::decoder::Decode;
use yrs::{ReadTxn, StateVector, Transact, Update};

use super::ports::{
    StorageAgentCatalog, WsContextSnapshot, WsDocuments, WsIntentions, WsLegalActEditor,
    WsMetadata, WsUserInteraction,
};
use super::presence::{color_for_id, display_initial};
use super::protocol::{
    AgentSessionEntryWire, AgentSessionWire, ClientMessage, DocumentUploadWire,
    InteractionAnswerWire, InteractionQuestionWire, PresenceUser, ServerMessage,
    SupervisorContextEntryWire, SupervisorContextToolCallWire,
};
use super::state::{EditorRoom, Presence};
use crate::auth::session::COOKIE_NAME;
use crate::state::{AppState, SessionKey};

const SUPERVISOR_SYSTEM_PROMPT: &str = "Tu es le superviseur d'une équipe d'agents experts qui \
    rédigent ensemble un arrêté préfectoral ICPE. Tu ne rédiges jamais toi-même le contenu de \
    l'acte : tu comprends la demande de l'inspecteur, tu consultes `read_structure`/`read_title` \
    pour connaître l'état actuel de l'acte, puis tu découpes la demande en sous-tâches précises \
    que tu délègues à l'expert approprié du catalogue via `delegate_to_expert`, en lui donnant une \
    description autonome et précise de ce qu'il doit faire (il ne voit pas cette conversation). \
    \n\n\
    Avant de commencer, relis toujours les métadonnées du projet avec `read_metadata` (clé \
    `todo_superviseur`) : si une todo-list y figure déjà avec une ou plusieurs sous-tâches encore \
    à `a_faire` (reprise après une tâche précédente interrompue ou une nouvelle demande qui \
    complète la précédente), poursuis les délégations à partir de cet état existant plutôt que de \
    tout redécomposer depuis zéro — n'ajoute que les sous-tâches réellement nouvelles à la liste. \
    Dès qu'une demande comporte plusieurs sous-tâches et qu'aucune todo-list exploitable n'existe \
    encore, tiens-en à jour une dans les métadonnées du projet (même clé `todo_superviseur`, \
    valeur : liste d'objets `{ tache, statut }`, `statut` valant `a_faire` ou `fait`) : écris-la \
    avec `write_metadata` juste après avoir décomposé la demande, avant la première délégation. \
    Après chaque réponse d'un expert délégué, relis-la avec `read_metadata` (même clé), fais \
    passer la ou les sous-tâches accomplies à `fait`, puis réécris la liste complète avec \
    `write_metadata` — jamais une sous-tâche oubliée ou laissée à `a_faire` sans délégation prévue \
    pour elle. Pour une demande ne comportant qu'une seule sous-tâche et sans todo-list existante, \
    la todo-list est facultative. \
    \n\n\
    Un expert peut lui-même poser une question à l'inspecteur si une information lui manque : dans \
    ce cas, attends simplement sa réponse avant de reprendre. Pose toi-même une question à \
    l'inspecteur (`ask_user`/`ask_questions`) uniquement pour clarifier la demande globale ou \
    trancher entre plusieurs experts possibles, jamais pour une question de détail rédactionnel qui \
    relève d'un expert. Les intentions rédactionnelles du projet (ex. « mise en demeure », « sanction \
    administrative ») s'ajoutent ou se retirent uniquement sur demande explicite de l'inspecteur, \
    avec `add_intention`/`remove_intention` : appelle d'abord `list_intentions` pour connaître les \
    intentions disponibles pour le domaine du projet et leur identifiant, n'en invente jamais un. \
    \n\n\
    Avant de conclure, si une todo-list a été ouverte, relis-la avec `read_metadata` (clé \
    `todo_superviseur`) : tant qu'une sous-tâche y figure encore à `a_faire`, poursuis les \
    délégations nécessaires plutôt que de t'arrêter. Une fois toutes les délégations nécessaires \
    terminées et la todo-list entièrement à `fait` (ou en l'absence de todo-list), résume en une \
    phrase ce qui a été fait.";

/// Outils directement accessibles au Superviseur (voir
/// [`SUPERVISOR_SYSTEM_PROMPT`]) : lecture/orientation, interaction, gestion
/// des intentions et délégation — jamais les outils de rédaction eux-mêmes
/// (`fill_section`, `insert_node`...) ni les API externes, réservés aux
/// profils d'experts du catalogue (voir `storage::agent_profile`). `write_metadata`
/// n'y figure que pour tenir la todo-list de suivi (clé `todo_superviseur`,
/// réservée au Superviseur) : il ne l'utilise jamais pour écrire une donnée
/// métier, qui reste du ressort des profils d'experts délégués.
const SUPERVISOR_TOOL_NAMES: &[&str] = &[
    "read_metadata",
    "write_metadata",
    "read_structure",
    "read_title",
    "list_intentions",
    "add_intention",
    "remove_intention",
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
    // Progression de l'agent (voir `super::state::EditorRoom::agent_events`) :
    // diffusée à toute la salle plutôt que réservée à la connexion qui a
    // démarré la tâche, pour qu'un rechargement de page ou un second onglet
    // continue de suivre une tâche déjà en cours (voir
    // `super::ports::WsUserInteraction`).
    let mut agent_rx = room.agent_events.subscribe();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();
    // Clones dédiés à la reconciliation après un décrochage de `agent_rx`
    // (voir `reconcile_agent_lag`) : `state`/`room_id` sont encore utilisés
    // après ce point par la boucle de lecture principale ci-dessous.
    let agent_lag_state = state.clone();
    let agent_lag_room_id = room_id.clone();

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
                text = agent_rx.recv() => match text {
                    Ok(text) => {
                        if sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Le message terminal (`AgentDone`/`AgentError`/pause)
                        // peut avoir été perdu avec les fragments
                        // intermédiaires sautés par ce décrochage, laissant le
                        // client bloqué sur « l'agent réfléchit » alors que la
                        // tâche est en fait déjà terminée côté serveur :
                        // interroge directement l'état persisté pour le
                        // débloquer (voir `reconcile_agent_lag`).
                        if let Some(message) =
                            reconcile_agent_lag(&agent_lag_state.store, &agent_lag_room_id).await
                            && let Ok(text) = serde_json::to_string(&message)
                            && sink.send(Message::Text(text.into())).await.is_err()
                        {
                            break;
                        }
                    }
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
    let selection: Arc<StdMutex<Option<NodeId>>> = Arc::new(StdMutex::new(None));
    let editor: Arc<dyn LegalActEditorPort> = Arc::new(WsLegalActEditor::new(
        room.clone(),
        selection.clone(),
        author_id,
    ));
    // Relaie les réflexions et appels d'outils de l'orchestration à tous les
    // pairs de la salle (voir `WsUserInteraction` dans `super::ports`, et
    // `EditorRoom::agent_events`).
    let agent_observer: Arc<dyn AgentObserver> = Arc::new(WsUserInteraction::new(
        room.agent_events.clone(),
    ));
    // Propre à cette connexion : voir la note sur son pendant, `selection`,
    // ci-dessus. Une orchestration reprise depuis une autre connexion après
    // une déconnexion repart avec `auto_accept = false`, ce qui est sans
    // risque (au pire, une confirmation de plus est demandée).
    let auto_accept = Arc::new(AtomicBool::new(false));

    // Une connexion qui arrive (nouvel onglet, reconnexion après coupure...)
    // doit voir immédiatement où l'orchestration en est, plutôt que de
    // paraître silencieusement bloquée : soit une question en attente (run
    // `paused`), soit un simple indicateur que la tâche est toujours en
    // cours (run `running`, dont la progression continuera d'arriver via
    // `EditorRoom::agent_events`, voir la boucle `send_task` ci-dessus).
    if let Ok(Some(run)) = storage::agent_run::get_active_run_for_room(&state.store, &room_id).await
    {
        if run.status == "paused" {
            if let Some(message) = replay_pending_interaction(&run) {
                let _ = out_tx.send(message);
            }
        } else {
            let _ = out_tx.send(ServerMessage::AgentRunInProgress);
        }
    }

    // Restaure le transcript de la session active pour que l'inspecteur
    // reprenne sa conversation là où il l'avait laissée après un
    // rechargement de page (voir [`restore_active_session`]), plutôt que de
    // retrouver un panneau vide et ne pouvoir consulter cette session que
    // depuis la liste, en lecture seule.
    if let Ok(Some(message)) = restore_active_session(&state.store, &room_id).await {
        let _ = out_tx.send(message);
    }

    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Binary(bytes) => apply_and_broadcast(&room, &author_id, &bytes).await,
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::RunAgent { task }) => {
                    let already_active =
                        storage::agent_run::get_active_run_for_room(&state.store, &room_id)
                            .await
                            .ok()
                            .flatten()
                            .is_some();
                    if !already_active {
                        spawn_agent_run(
                            state.clone(),
                            room.clone(),
                            editor.clone(),
                            agent_observer.clone(),
                            room.agent_events.clone(),
                            auto_accept.clone(),
                            room_id.clone(),
                            room.legal_act_id(),
                            author_id,
                            AgentInput::Start { task },
                        );
                    }
                }
                Ok(ClientMessage::InteractionAnswer { value }) => {
                    let paused =
                        storage::agent_run::get_active_run_for_room(&state.store, &room_id)
                            .await
                            .ok()
                            .flatten()
                            .is_some_and(|run| run.status == "paused");
                    if paused {
                        spawn_agent_run(
                            state.clone(),
                            room.clone(),
                            editor.clone(),
                            agent_observer.clone(),
                            room.agent_events.clone(),
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
                        .and_then(|raw| raw.parse::<NodeId>().ok());
                    *selection.lock().expect("verrou non empoisonné") = parsed;
                }
                Ok(ClientMessage::ClearHistory) => {
                    let active =
                        storage::agent_run::get_active_run_for_room(&state.store, &room_id)
                            .await
                            .ok()
                            .flatten()
                            .is_some();
                    if !active {
                        let _ = storage::agent_session::archive_active_session_for_room(
                            &state.store,
                            &room_id,
                        )
                        .await;
                    }
                }
                Ok(ClientMessage::ListAgentSessions) => {
                    if let Ok(message) =
                        list_agent_sessions(&state.store, &room_id, &author_id).await
                    {
                        let _ = out_tx.send(message);
                    }
                }
                Ok(ClientMessage::GetAgentSessionHistory { session_id }) => {
                    if let Ok(message) =
                        get_agent_session_history(&state.store, &room_id, session_id).await
                    {
                        let _ = out_tx.send(message);
                    }
                }
                Ok(ClientMessage::ReviewUpdate { update }) => {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(update) {
                        apply_and_broadcast_review(&room, &author_id, &bytes).await;
                    }
                }
                Ok(ClientMessage::GetSupervisorContext) => {
                    if let Ok(message) = get_supervisor_context(&state.store, &room_id).await {
                        let _ = out_tx.send(message);
                    }
                }
                Ok(ClientMessage::StopAgent) => {
                    if let Ok(Some(run)) =
                        storage::agent_run::get_active_run_for_room(&state.store, &room_id).await
                    {
                        if let Some(handle) = room.take_agent_task() {
                            handle.abort();
                        }
                        // Sans effet (et sans diffusion) si le run est passé
                        // entretemps à `done`/`failed` par la tâche elle-même
                        // (`stop_run` échoue alors silencieusement, voir sa
                        // documentation) : le message terminal légitime a
                        // déjà été diffusé par cette tâche, un `AgentStopped`
                        // ferait ici plus de mal que de bien.
                        if storage::agent_run::stop_run(&state.store, &run.id, run.version)
                            .await
                            .is_ok()
                            && let Ok(text) = serde_json::to_string(&ServerMessage::AgentStopped)
                        {
                            let _ = room.agent_events.send(text);
                        }
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

/// Construit un [`LanguageModel`] prêt à l'emploi à partir d'un
/// [`shared::model::AiModel`] enregistré, en déchiffrant sa clé API avec
/// `AppState::secret_encryption_key` — partagé par [`build_active_language_model`]
/// (modèle par défaut) et [`build_agent_profile_models`] (modèles dédiés à
/// certains profils d'experts, voir `/admin/agent-profiles`).
fn build_language_model(
    ai_model: shared::model::AiModel,
    secret_key: &Option<Vec<u8>>,
) -> Result<Arc<dyn LanguageModel>, String> {
    let key = secret_key.as_ref().ok_or_else(|| {
        "SECRET_ENCRYPTION_KEY absente : impossible de déchiffrer la clé API du modèle IA"
            .to_string()
    })?;
    let api_key = shared::crypto::decrypt(key, &ai_model.api_key_encrypted)
        .map_err(|_| "échec du déchiffrement de la clé API du modèle IA".to_string())?;

    Ok(Arc::new(OpenAiCompatibleModel::new(
        OpenAiCompatibleModelConfig {
            base_url: ai_model.base_url,
            api_key,
            model: ai_model.model,
        },
    )))
}

/// Résout le modèle IA actif (voir `/admin/ai-models`,
/// `storage::ai_model::get_active_ai_model`) en un [`LanguageModel`] prêt à
/// l'emploi : le modèle par défaut de l'Orchestrateur, utilisé par le
/// Superviseur et tout expert dont le profil ne dédie pas de modèle
/// spécifique (voir [`build_agent_profile_models`]).
///
/// Échoue avec un message destiné à l'utilisateur si aucun modèle n'est actif
/// ou si sa clé API ne peut pas être déchiffrée : contrairement au contexte de
/// domaine/intentions, l'absence de modèle empêche réellement de lancer
/// l'orchestration.
async fn build_active_language_model(
    pool: &storage::Pool,
    secret_key: &Option<Vec<u8>>,
) -> Result<(Arc<dyn LanguageModel>, String), String> {
    let ai_model = storage::ai_model::get_active_ai_model(pool)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| {
            "aucun modèle IA actif n'est configuré (voir /admin/ai-models)".to_string()
        })?;
    let system_prompt = ai_model.system_prompt.clone();
    let model = build_language_model(ai_model, secret_key)?;
    Ok((model, system_prompt))
}

/// Résout, pour chaque profil d'expert de `profiles` dédiant un modèle
/// spécifique (voir `agent::AgentProfile::model_id`), le [`LanguageModel`]
/// correspondant — indexé par ce même identifiant, prêt à être passé à
/// [`Orchestrator::new`]. Un profil dont le modèle référencé a depuis été
/// supprimé, ou dont la clé API ne peut pas être déchiffrée, est simplement
/// absent de la table renvoyée plutôt que de faire échouer toute
/// l'orchestration : [`agent::orchestration::Orchestrator::model_for`] retombe
/// alors sur le modèle par défaut pour cet expert.
async fn build_agent_profile_models(
    pool: &storage::Pool,
    secret_key: &Option<Vec<u8>>,
    profiles: &[agent::AgentProfile],
) -> std::collections::HashMap<String, Arc<dyn LanguageModel>> {
    let mut models = std::collections::HashMap::new();
    for model_id in profiles
        .iter()
        .filter_map(|profile| profile.model_id.as_deref())
        .collect::<HashSet<_>>()
    {
        let Ok(id) = model_id.parse::<ID>() else {
            continue;
        };
        let Ok(ai_model) = storage::ai_model::get_ai_model(pool, &id).await else {
            continue;
        };
        if let Ok(model) = build_language_model(ai_model, secret_key) {
            models.insert(model_id.to_string(), model);
        }
    }
    models
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
        PauseRequest::Ask { question } => ServerMessage::InteractionAsk {
            agent_label,
            question,
        },
        PauseRequest::Confirm { message } => ServerMessage::InteractionConfirm {
            agent_label,
            message,
        },
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

/// Reconstitue un message de synchronisation à envoyer au client lorsque son
/// abonnement à [`super::state::EditorRoom::agent_events`] a décroché
/// (`RecvError::Lagged`, voir [`handle_socket`]) : un burst de fragments de
/// réflexion/contenu peut dépasser la capacité du canal de diffusion, auquel
/// cas le message terminal (`AgentDone`/`AgentError`/pause) qu'il portait a
/// pu être perdu avec les fragments intermédiaires sautés, laissant le client
/// bloqué indéfiniment sur « l'agent réfléchit » alors que la tâche est en
/// fait déjà terminée côté serveur. Les fragments perdus eux-mêmes ne sont
/// pas récupérables (non journalisés) ; interroge directement l'état persisté
/// pour au moins débloquer l'indicateur d'attente et, le cas échéant, rejouer
/// une interaction en attente. `None` si rien de pertinent n'est à renvoyer
/// (run toujours `running` : la suite continuera d'arriver normalement).
async fn reconcile_agent_lag(pool: &storage::Pool, room_id: &str) -> Option<ServerMessage> {
    match storage::agent_run::get_active_run_for_room(pool, room_id)
        .await
        .ok()?
    {
        Some(run) if run.status == "paused" => replay_pending_interaction(&run),
        Some(_running) => None,
        None => {
            let session = storage::agent_session::get_active_session_for_room(pool, room_id)
                .await
                .ok()??;
            let run = storage::agent_run::get_latest_run_for_session(pool, &session.id)
                .await
                .ok()??;
            Some(match run.status.as_str() {
                "failed" => ServerMessage::AgentError {
                    message: "une erreur est survenue pendant le traitement de la tâche"
                        .to_string(),
                },
                "stopped" => ServerMessage::AgentStopped,
                _ => ServerMessage::AgentDone,
            })
        }
    }
}

/// Tronque `s` à `max` caractères (sur les `char`, pas les octets) pour
/// l'aperçu d'une session dans la liste (voir [`list_agent_sessions`]), avec
/// une ellipse si nécessaire.
fn truncate_preview(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Premier message utilisateur de l'historique du frame Superviseur, tronqué
/// pour servir d'aperçu à une session dans la liste (voir
/// [`list_agent_sessions`]) : `None` si l'historique n'en contient aucun (ne
/// devrait pas arriver, tout frame racine démarre par un message utilisateur,
/// voir `agent::orchestration::AgentFrame::new`).
fn first_user_message_preview(history: &[ChatMessage]) -> Option<String> {
    history
        .iter()
        .find(|message| message.role == Role::User)
        .and_then(|message| message.content.as_deref())
        .map(|content| truncate_preview(content, 80))
}

/// Construit la réponse à [`ClientMessage::ListAgentSessions`] : les sessions
/// de `author_id` pour `room_id`, chacune accompagnée d'un aperçu de son
/// premier message (voir [`first_user_message_preview`]) quand son run le
/// plus ancien reste interprétable.
async fn list_agent_sessions(
    pool: &storage::Pool,
    room_id: &str,
    author_id: &ID,
) -> Result<ServerMessage, storage::StorageError> {
    let sessions = storage::agent_session::list_sessions_for_room(pool, room_id, author_id).await?;
    let mut wire = Vec::with_capacity(sessions.len());
    for session in sessions {
        let preview = storage::agent_run::get_earliest_run_for_session(pool, &session.id)
            .await
            .ok()
            .flatten()
            .and_then(|run| serde_json::from_value::<Vec<AgentFrame>>(run.stack).ok())
            .and_then(|stack| {
                stack
                    .first()
                    .and_then(|frame| first_user_message_preview(&frame.history))
            });
        wire.push(AgentSessionWire {
            id: session.id.to_string(),
            status: session.status,
            created_at: session.created_at.to_rfc3339(),
            archived_at: session.archived_at.map(|at| at.to_rfc3339()),
            preview,
        });
    }
    Ok(ServerMessage::AgentSessions { sessions: wire })
}

/// Traduit l'historique `agent::ChatMessage` du frame Superviseur en
/// transcript affichable (voir [`AgentSessionEntryWire`]) : les messages
/// `system` sont omis (détail d'implémentation, jamais montré à
/// l'inspecteur), un message assistant sans contenu textuel (appel d'outil
/// pur) n'a pas d'entrée propre — c'est le résultat d'outil qui en porte une,
/// une fois son appel d'origine retrouvé par `tool_call_id`.
fn agent_session_history_from_chat_messages(history: &[ChatMessage]) -> Vec<AgentSessionEntryWire> {
    let mut pending_calls: std::collections::HashMap<String, (String, serde_json::Value)> =
        std::collections::HashMap::new();
    let mut entries = Vec::new();

    for message in history {
        match message.role {
            Role::System => {}
            Role::User => {
                if let Some(content) = &message.content {
                    entries.push(AgentSessionEntryWire::User {
                        content: content.clone(),
                    });
                }
            }
            Role::Assistant => {
                if let Some(content) = &message.content {
                    entries.push(AgentSessionEntryWire::Assistant {
                        content: content.clone(),
                    });
                }
                for call in &message.tool_calls {
                    pending_calls
                        .insert(call.id.clone(), (call.name.clone(), call.arguments.clone()));
                }
            }
            Role::Tool => {
                let Some(call_id) = &message.tool_call_id else {
                    continue;
                };
                let Some((name, arguments)) = pending_calls.remove(call_id) else {
                    continue;
                };
                entries.push(AgentSessionEntryWire::ToolCall {
                    name,
                    arguments,
                    output: message.content.clone().unwrap_or_default(),
                });
            }
        }
    }

    entries
}

/// Traduit l'historique complet `agent::ChatMessage` du frame Superviseur en
/// contexte brut affichable (voir [`ServerMessage::SupervisorContext`]) :
/// contrairement à [`agent_session_history_from_chat_messages`], le message
/// système est conservé et un appel d'outil n'est jamais fusionné avec son
/// résultat — chaque message de l'historique produit sa propre entrée, dans
/// l'ordre, pour refléter tel quel ce que le Superviseur envoie effectivement
/// au modèle.
fn supervisor_context_from_chat_messages(
    history: &[ChatMessage],
) -> Vec<SupervisorContextEntryWire> {
    history
        .iter()
        .map(|message| match message.role {
            Role::System => SupervisorContextEntryWire::System {
                content: message.content.clone().unwrap_or_default(),
            },
            Role::User => SupervisorContextEntryWire::User {
                content: message.content.clone().unwrap_or_default(),
            },
            Role::Assistant => SupervisorContextEntryWire::Assistant {
                content: message.content.clone(),
                tool_calls: message
                    .tool_calls
                    .iter()
                    .map(|call| SupervisorContextToolCallWire {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: serde_json::to_string_pretty(&call.arguments)
                            .unwrap_or_else(|_| call.arguments.to_string()),
                    })
                    .collect(),
            },
            Role::Tool => SupervisorContextEntryWire::ToolResult {
                tool_call_id: message.tool_call_id.clone().unwrap_or_default(),
                content: message.content.clone().unwrap_or_default(),
            },
        })
        .collect()
}

/// Construit la réponse à [`ClientMessage::GetSupervisorContext`] : contexte
/// brut de l'historique du frame Superviseur du run le plus récent de
/// `room_id` (actif s'il y en a un, sinon celui de la session active) — ce
/// contexte n'a de sens que pour la conversation la plus récente, celle que
/// l'inspecteur peut effectivement influencer, contrairement aux sessions
/// archivées consultables via [`ClientMessage::GetAgentSessionHistory`].
/// Renvoie une liste vide si la salle n'a encore aucun run.
async fn get_supervisor_context(
    pool: &storage::Pool,
    room_id: &str,
) -> Result<ServerMessage, String> {
    let run = match storage::agent_run::get_active_run_for_room(pool, room_id)
        .await
        .map_err(|error| error.to_string())?
    {
        Some(run) => Some(run),
        None => match storage::agent_session::get_active_session_for_room(pool, room_id)
            .await
            .map_err(|error| error.to_string())?
        {
            Some(session) => storage::agent_run::get_latest_run_for_session(pool, &session.id)
                .await
                .map_err(|error| error.to_string())?,
            None => None,
        },
    };

    let entries = match run {
        Some(run) => {
            let stack: Vec<AgentFrame> =
                serde_json::from_value(run.stack).map_err(|error| error.to_string())?;
            stack
                .first()
                .map(|frame| supervisor_context_from_chat_messages(&frame.history))
                .unwrap_or_default()
        }
        None => Vec::new(),
    };

    Ok(ServerMessage::SupervisorContext { entries })
}

/// Reconstruit le transcript de la session active de `room_id` (s'il y en a
/// une), pour le renvoyer à une connexion qui vient de s'ouvrir (voir
/// [`handle_socket`]) : contrairement à [`get_agent_session_history`], ce
/// transcript alimente la conversation affichée elle-même
/// (`ServerMessage::AgentActiveSession`), pas un recouvrement en lecture
/// seule — c'est ce qui permet à l'inspecteur de reprendre sa conversation
/// après un rechargement de page plutôt que de la retrouver vide, la session
/// active n'étant autrement consultable qu'en lecture seule via
/// [`ClientMessage::GetAgentSessionHistory`].
async fn restore_active_session(
    pool: &storage::Pool,
    room_id: &str,
) -> Result<Option<ServerMessage>, String> {
    let Some(session) = storage::agent_session::get_active_session_for_room(pool, room_id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };

    let entries = match storage::agent_run::get_latest_run_for_session(pool, &session.id)
        .await
        .map_err(|error| error.to_string())?
    {
        Some(run) => {
            let stack: Vec<AgentFrame> =
                serde_json::from_value(run.stack).map_err(|error| error.to_string())?;
            stack
                .first()
                .map(|frame| agent_session_history_from_chat_messages(&frame.history))
                .unwrap_or_default()
        }
        None => Vec::new(),
    };

    if entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(ServerMessage::AgentActiveSession { entries }))
}

/// Construit la réponse à [`ClientMessage::GetAgentSessionHistory`] : vérifie
/// que `session_id` appartient bien à `room_id` (une session d'une autre
/// salle ne doit jamais être consultable depuis celle-ci) avant de
/// reconstruire son transcript depuis le run le plus récent qui lui est
/// rattaché.
async fn get_agent_session_history(
    pool: &storage::Pool,
    room_id: &str,
    session_id: String,
) -> Result<ServerMessage, String> {
    let id: ID = session_id.parse().map_err(|error| format!("{error}"))?;
    let session = storage::agent_session::get_session(pool, &id)
        .await
        .map_err(|error| error.to_string())?
        .filter(|session| session.room_id == room_id)
        .ok_or_else(|| "session introuvable pour cette salle".to_string())?;

    let entries = match storage::agent_run::get_latest_run_for_session(pool, &session.id)
        .await
        .map_err(|error| error.to_string())?
    {
        Some(run) => {
            let stack: Vec<AgentFrame> =
                serde_json::from_value(run.stack).map_err(|error| error.to_string())?;
            stack
                .first()
                .map(|frame| agent_session_history_from_chat_messages(&frame.history))
                .unwrap_or_default()
        }
        None => Vec::new(),
    };

    Ok(ServerMessage::AgentSessionHistory {
        session_id,
        entries,
    })
}

/// Convertit la réponse brute du client (`ClientMessage::InteractionAnswer`)
/// en [`PauseAnswer`] adaptée à `request`, en persistant au passage les
/// octets d'un document uploadé (voir `storage::legal_act_document::store_document`)
/// pour qu'il survive à la connexion courante, rattaché au projet
/// `legal_act_id` plutôt qu'au run en cours — au même titre qu'un document
/// ajouté depuis le panneau « Fichiers » (voir
/// `app::pages::project_documents::upload_project_document`).
async fn decode_pause_answer(
    pool: &storage::Pool,
    legal_act_id: Option<ID>,
    uploaded_by: &ID,
    agent_events: &tokio::sync::broadcast::Sender<String>,
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
        PauseRequest::RequestDocument { prompt, .. } => {
            let legal_act_id = legal_act_id
                .ok_or_else(|| "aucun projet associé à cette salle".to_string())?;
            let upload: DocumentUploadWire = serde_json::from_value(value)
                .map_err(|error| format!("réponse invalide à la demande de document : {error}"))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&upload.content_base64)
                .map_err(|error| format!("contenu du document invalide (base64) : {error}"))?;
            // Le libellé du document reprend la description que l'agent
            // avait lui-même donnée à sa demande (`prompt`, ex. « le rapport
            // d'inspection ICPE le plus récent ») : c'est exactement l'alias
            // sémantique dont un futur `search_documents` a besoin, sans
            // qu'il faille demander à l'inspecteur de le ressaisir.
            let document = storage::legal_act_document::store_document(
                pool,
                &legal_act_id,
                &upload.file_name,
                &upload.mime_type,
                bytes,
                prompt,
                uploaded_by,
            )
            .await
            .map_err(|error| error.to_string())?;

            if let Ok(text) = serde_json::to_string(&ServerMessage::DocumentsChanged(
                DocumentsChangedEvent {
                    file_name: document.file_name.clone(),
                    kind: DocumentChangeKind::Uploaded,
                    by_agent: true,
                    actor_id: None,
                },
            )) {
                let _ = agent_events.send(text);
            }

            Ok(PauseAnswer::Document(agent::ports::DocumentRef {
                id: document.id.to_string(),
                file_name: document.file_name,
                mime_type: document.mime_type,
                label: document.label,
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
        Ok(RunOutcome::Paused {
            agent_label,
            request,
        }) => pause_request_to_server_message(agent_label, request),
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

    match storage::agent_run::save_run(
        pool,
        run_id,
        version,
        status,
        stack,
        run.final_answer.as_deref(),
    )
    .await
    {
        Ok(_) => message,
        Err(error) => ServerMessage::AgentError {
            message: format!("échec de la sauvegarde de l'orchestration : {error}"),
        },
    }
}

/// Démarre l'orchestration sur une tâche Tokio détachée : son issue finale
/// (`AgentDone`/`AgentError`/pause) est diffusée sur `agent_events` (voir
/// `EditorRoom::agent_events`) plutôt qu'envoyée à une connexion précise, pour
/// que tout pair encore connecté à ce moment-là la reçoive — y compris une
/// connexion qui aurait rejoint la salle après le démarrage de cette tâche
/// (nouvel onglet, reconnexion après un rechargement de page). Sans effet
/// visible immédiat si personne n'est connecté à cet instant : l'issue reste
/// de toute façon persistée (voir [`persist_run_outcome`]) et rejouée à la
/// prochaine connexion (voir [`restore_active_session`]/
/// [`replay_pending_interaction`]).
fn spawn_agent_run(
    state: Arc<AppState>,
    room: Arc<EditorRoom>,
    editor: Arc<dyn LegalActEditorPort>,
    agent_observer: Arc<dyn AgentObserver>,
    agent_events: tokio::sync::broadcast::Sender<String>,
    auto_accept: Arc<AtomicBool>,
    room_id: String,
    legal_act_id: Option<ID>,
    author_id: ID,
    input: AgentInput,
) {
    let room_for_task = room.clone();
    let handle = tokio::spawn(async move {
        let message = run_orchestration(
            &state,
            editor,
            agent_observer,
            agent_events.clone(),
            auto_accept,
            &room_id,
            legal_act_id,
            author_id,
            input,
        )
        .await
        .unwrap_or_else(|message| ServerMessage::AgentError { message });
        // Retire sa propre poignée d'annulation avant de diffuser l'issue :
        // une fois celle-ci enregistrée `None`, un `StopAgent` concurrent sait
        // qu'il n'y a plus rien à interrompre (voir
        // `EditorRoom::take_agent_task`). Sans effet si `StopAgent` l'a déjà
        // retirée entretemps.
        room_for_task.take_agent_task();
        if let Ok(text) = serde_json::to_string(&message) {
            let _ = agent_events.send(text);
        }
    });
    room.set_agent_task(handle.abort_handle());
}

/// Construit l'orchestrateur (modèle, registre d'outils complet, catalogue
/// d'experts) puis démarre ou reprend un run selon `input`, et persiste son
/// issue (voir [`persist_run_outcome`]).
async fn run_orchestration(
    state: &Arc<AppState>,
    editor: Arc<dyn LegalActEditorPort>,
    agent_observer: Arc<dyn AgentObserver>,
    agent_events: tokio::sync::broadcast::Sender<String>,
    auto_accept: Arc<AtomicBool>,
    room_id: &str,
    legal_act_id: Option<ID>,
    author_id: ID,
    input: AgentInput,
) -> Result<ServerMessage, String> {
    let secret_key = state.secret_encryption_key.clone();
    let (model, ai_model_system_prompt) =
        build_active_language_model(&state.store, &secret_key).await?;
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
    tools.register(Box::new(FetchDocumentByUrlTool::new()));

    let mut context_snapshot: Option<Arc<dyn agent::ports::ContextSnapshotPort>> = None;

    if let Some(legal_act_id) = legal_act_id {
        context_snapshot = Some(Arc::new(WsContextSnapshot::new(
            state.store.clone(),
            legal_act_id,
        )));

        let intentions: Arc<dyn agent::ports::IntentionPort> = Arc::new(WsIntentions::new(
            state.store.clone(),
            legal_act_id,
            author_id,
        ));
        tools.register(Box::new(ListIntentionsTool::new(intentions.clone())));
        tools.register(Box::new(AddIntentionTool::new(intentions.clone())));
        tools.register(Box::new(RemoveIntentionTool::new(intentions)));

        let metadata: Arc<dyn agent::ports::MetadataPort> = Arc::new(WsMetadata::new(
            state.store.clone(),
            legal_act_id,
            author_id,
            agent_events.clone(),
        ));
        tools.register(Box::new(ReadMetadataTool::new(metadata.clone())));
        tools.register(Box::new(WriteMetadataTool::new(metadata.clone(), true)));
        tools.register(Box::new(SearchMetadataTool::new(metadata)));

        // Les documents du projet sont désormais rattachés à `legal_act_id`
        // plutôt qu'à une session de conversation (voir
        // `storage::legal_act_document`) : sans projet résolu pour cette
        // salle, ni `read_document` ni `search_documents` ne peuvent
        // fonctionner (`request_document`/`fetch_document_by_url` restent
        // disponibles, la première ne faisant que suspendre l'orchestration,
        // la seconde n'ayant aucune dépendance au projet).
        let documents: Arc<dyn agent::ports::DocumentContentPort> =
            Arc::new(WsDocuments::new(state.store.clone(), legal_act_id));
        tools.register(Box::new(SearchDocumentsTool::new(Some(documents.clone()))));
        tools.register(Box::new(ReadDocumentTool::new(Some(documents))));
    }

    if allowed_tools.contains("georisques_query") || allowed_tools.contains("icpe_query") {
        let georisques_client =
            Arc::new(build_georisques_client(&state.store, secret_key.clone()).await);
        if allowed_tools.contains("georisques_query") {
            tools.register(Box::new(GeorisquesQueryTool::new(
                georisques_client.clone(),
            )));
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
                tools.register(Box::new(LegifranceSearchTool::new(
                    legifrance_client.clone(),
                )));
            }
            if allowed_tools.contains("legifrance_fetch") {
                tools.register(Box::new(LegifranceFetchTool::new(legifrance_client)));
            }
        }
    }

    let catalog = StorageAgentCatalog::new(state.store.clone());
    let profiles = catalog.list().await.map_err(|error| error.to_string())?;
    tools.register(Box::new(DelegateToExpertTool::new(&profiles)));
    // Disponible pour tout profil d'expert dont `tool_names` (voir
    // `/admin/agent-profiles`) l'inclut : contrairement à
    // `delegate_to_expert`, réservé au Superviseur (voir
    // `SUPERVISOR_TOOL_NAMES`), `spawn_expert` permet à un expert en cours de
    // tâche de confier une sous-tâche dynamique à une nouvelle instance du
    // Superviseur plutôt que de choisir lui-même un profil du catalogue.
    tools.register(Box::new(SpawnExpertTool));

    // Certains profils d'experts (voir `/admin/agent-profiles`) dédient un
    // modèle différent du modèle actif par défaut, pour tirer parti des
    // forces propres à chaque modèle selon la sous-tâche déléguée.
    let agent_profile_models = build_agent_profile_models(&state.store, &secret_key, &profiles).await;

    let mut orchestrator = Orchestrator::new(
        model,
        agent_profile_models,
        tools,
        Arc::new(catalog),
        agent_observer,
        auto_accept,
    );
    if let Some(context_snapshot) = context_snapshot {
        orchestrator = orchestrator.with_context_snapshot(context_snapshot);
    }

    match input {
        AgentInput::Start { task } => {
            let session =
                match storage::agent_session::get_active_session_for_room(&state.store, room_id)
                    .await
                    .map_err(|error| error.to_string())?
                {
                    Some(session) => session,
                    None => {
                        storage::agent_session::create_session(&state.store, room_id, &author_id)
                            .await
                            .map_err(|error| error.to_string())?
                    }
                };

            let root =
                match storage::agent_run::get_latest_run_for_session(&state.store, &session.id)
                    .await
                    .ok()
                    .flatten()
                {
                    Some(previous) if previous.status == "done" => {
                        let mut stack: Vec<AgentFrame> = serde_json::from_value(previous.stack)
                            .map_err(|error| error.to_string())?;
                        let root = stack.pop().ok_or_else(|| {
                            "run précédent terminé sans frame superviseur".to_string()
                        })?;
                        root.resume_as_new_task(&task)
                    }
                    _ => AgentFrame::supervisor(
                        system_prompt,
                        SUPERVISOR_TOOL_NAMES
                            .iter()
                            .map(|name| name.to_string())
                            .collect(),
                        SUPERVISOR_MAX_STEPS,
                        &task,
                    ),
                };

            let mut run = OrchestrationRun::new(root);
            let initial_stack =
                serde_json::to_value(&run.stack).map_err(|error| error.to_string())?;
            let created = storage::agent_run::create_run(
                &state.store,
                room_id,
                &session.id,
                &author_id,
                initial_stack,
            )
            .await
            .map_err(|error| error.to_string())?;

            let drive_result = orchestrator.drive(&mut run).await;
            Ok(persist_run_outcome(
                &state.store,
                &created.id,
                created.version,
                run,
                drive_result,
            )
            .await)
        }
        AgentInput::Resume { value } => {
            let existing = storage::agent_run::get_active_run_for_room(&state.store, room_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "aucune tâche en attente sur cette salle".to_string())?;
            let stack: Vec<AgentFrame> = serde_json::from_value(existing.stack.clone())
                .map_err(|error| error.to_string())?;
            let pending_request = stack
                .last()
                .and_then(|frame| frame.pending.as_ref())
                .and_then(|pending| match &pending.reason {
                    PauseReason::Interaction(request) => Some(request.clone()),
                    PauseReason::Delegating => None,
                })
                .ok_or_else(|| "cette salle n'est pas en attente d'une réponse".to_string())?;

            let answer = decode_pause_answer(
                &state.store,
                legal_act_id,
                &author_id,
                &agent_events,
                &pending_request,
                value,
            )
            .await?;
            let mut run = OrchestrationRun {
                stack,
                status: RunStatus::Paused,
                final_answer: None,
            };
            let drive_result = orchestrator.resume(&mut run, answer).await;
            Ok(persist_run_outcome(
                &state.store,
                &existing.id,
                existing.version,
                run,
                drive_result,
            )
            .await)
        }
    }
}
