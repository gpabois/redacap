//! Handler websocket de collaboration : chaque connexion rejoint la
//! [`Room`] identifiée par `room_id` dans l'URL (`/ws/{room_id}`).
//!
//! Deux types de trames y transitent :
//! - des trames **binaires**, qui portent des mises à jour Yrs brutes
//!   (encodées via `encode_diff_v1`/`encode_state_as_update_v1`) : celles
//!   reçues d'un client sont appliquées au [`YrsBody`] partagé puis
//!   rediffusées à tous les autres pairs de la salle ;
//! - des trames **texte** JSON ([`ClientMessage`]/[`ServerMessage`]), qui
//!   pilotent la boucle agentique (`run_agent`) et relaient les
//!   interactions qu'elle déclenche (`ask_user`, `ask_questions`...), ainsi
//!   que les changements de présence ([`ServerMessage::Presence`]).
//!
//! Quand un outil de l'agent modifie le corps de l'acte (ex: `fill_section`),
//! la mise à jour Yrs qui en résulte est diffusée de la même façon qu'une
//! édition utilisateur : tous les clients convergent vers le même document.
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

use agent::ports::{LegalActEditorPort, UserInteractionPort};
use agent::tools::{
    AddIntentionTool, AskQuestionsTool, AskUserTool, FillSectionTool, GenerateNumberingTool,
    GeorisquesClient, GeorisquesConfig, GeorisquesQueryTool, IcpeQueryTool, InsertNodeTool,
    LegifranceClient, LegifranceConfig, LegifranceFetchTool, LegifranceSearchTool,
    ListIntentionsTool, ReadStructureTool, ReadTitleTool, RemoveIntentionTool, RemoveNodeTool,
    SetTitleTool, ValidateStructureTool,
};
use agent::{
    Agent, AgentConfig, LanguageModel, OpenAiCompatibleModel, OpenAiCompatibleModelConfig,
    ToolRegistry,
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

use super::ports::{WsIntentions, WsLegalActEditor, WsUserInteraction};
use super::presence::{color_for_id, display_initial};
use super::protocol::{ClientMessage, PresenceUser, ServerMessage};
use super::state::{EditorRoom, Presence};
use crate::auth::session::COOKIE_NAME;
use crate::state::{AppState, SessionKey};

const AGENT_SYSTEM_PROMPT: &str = "Tu es un agent IA qui aide à rédiger un arrêté préfectoral \
    ICPE. Utilise les outils à ta disposition pour compléter l'acte en cours d'édition ; \
    demande confirmation avant toute modification irréversible et pose des questions à \
    l'inspecteur quand une information te manque. Avant toute opération qui dépend du contenu \
    déjà rédigé (renumérotation, réécriture d'un libellé existant, vérification d'un doublon...), \
    appelle `read_structure` pour lire l'acte tel qu'il est actuellement : ne demande jamais à \
    l'inspecteur de te fournir ou de copier-coller un texte que tu peux lire toi-même avec cet \
    outil. Les outils `fill_section`, `insert_node` et `remove_node` attendent un identifiant de \
    nœud : n'en invente jamais un et ne demande jamais à l'inspecteur de t'en fournir un — \
    utilise le mot-clé « root » pour viser la racine de l'acte (ex. pour y insérer un premier \
    visa, considérant ou article), le mot-clé « selection » pour viser le nœud que l'inspecteur a \
    ciblé dans l'éditeur, ou bien l'identifiant renvoyé par un appel précédent à `insert_node`, ou \
    encore l'un des identifiants renvoyés par `read_structure`. Le titre de l'acte (son intitulé, \
    ex. « Arrêté préfectoral portant autorisation d'exploiter... ») se lit et se modifie avec \
    `read_title`/`set_title` : ne le confonds pas avec les nœuds `Titre` du corps, qui sont des \
    subdivisions numérotées (« Titre I », « Titre II »...) créées via `insert_node`. Les \
    intentions rédactionnelles du projet (ex. « mise en demeure », « sanction administrative ») \
    s'ajoutent ou se retirent uniquement sur demande explicite de l'inspecteur, avec \
    `add_intention`/`remove_intention` : appelle d'abord `list_intentions` pour connaître les \
    intentions disponibles pour le domaine du projet et leur identifiant, n'en invente jamais un.";

/// Complète [`AGENT_SYSTEM_PROMPT`] avec le prompt système dédié du modèle IA
/// actif (voir `shared::model::AiModel::system_prompt`, ajouté en entête),
/// puis avec le contexte du domaine du projet et des intentions qui lui sont
/// associées (voir `Claude.md` § « Ajoute aux projets... »), et résout
/// l'ensemble des outils autorisés pour ce domaine (voir
/// `storage::agent_tool_scope::list_allowed_tool_names_for_domain`).
///
/// Renvoie le prompt de base (+ prompt du modèle) seul et un ensemble vide si
/// `legal_act_id` est absent ou si le projet, son domaine ou ses intentions ne
/// peuvent pas être chargés : une erreur ici ne doit jamais empêcher de
/// lancer la boucle agentique, seulement priver le prompt de contexte
/// additionnel.
async fn build_agent_context(
    pool: &storage::Pool,
    legal_act_id: Option<ID>,
    ai_model_system_prompt: &str,
) -> (String, HashSet<String>) {
    let mut prompt = AGENT_SYSTEM_PROMPT.to_string();
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
    let (answer_tx, answer_rx) = mpsc::unbounded_channel::<serde_json::Value>();

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
    // Une seule instance concrète, utilisée à la fois comme port
    // d'interaction et comme observateur de la boucle agentique : les deux
    // rôles relaient vers le même client websocket (voir `WsUserInteraction`
    // dans `super::ports`).
    let ws_interaction = Arc::new(WsUserInteraction::new(out_tx.clone(), answer_rx));
    let interaction: Arc<dyn UserInteractionPort> = ws_interaction.clone();
    let agent_observer: Arc<dyn agent::AgentObserver> = ws_interaction;
    let agent_running = Arc::new(AtomicBool::new(false));
    // Persiste pour toute la durée de la connexion (pas seulement une tâche
    // agent) : l'utilisateur peut activer/désactiver l'auto-acceptation
    // indépendamment de `RunAgent`, y compris pendant qu'une tâche est en
    // cours (voir `agent::Agent::dispatch_tool_call`, qui le relit à chaque
    // appel d'outil plutôt qu'à la construction).
    let auto_accept = Arc::new(AtomicBool::new(false));
    // Historique de la conversation avec l'agent, conservé pour toute la
    // durée de la connexion : chaque `RunAgent` reconstruit un `Agent` (les
    // outils disponibles peuvent changer entre deux tâches), mais doit
    // reprendre la conversation là où elle s'est arrêtée, sans quoi l'agent
    // oublie les échanges précédents dès le message suivant (voir
    // `agent::Agent::run`).
    let agent_history: Arc<tokio::sync::Mutex<Vec<agent::ChatMessage>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Binary(bytes) => apply_and_broadcast(&room, &author_id, &bytes).await,
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::RunAgent { task }) => {
                    if agent_running.swap(true, Ordering::SeqCst) {
                        // Une tâche est déjà en cours sur cette connexion : ignorée.
                        continue;
                    }
                    spawn_agent_run(
                        state.clone(),
                        editor.clone(),
                        interaction.clone(),
                        agent_observer.clone(),
                        out_tx.clone(),
                        agent_running.clone(),
                        auto_accept.clone(),
                        agent_history.clone(),
                        room.legal_act_id(),
                        author_id,
                        task,
                    );
                }
                Ok(ClientMessage::InteractionAnswer { value }) => {
                    let _ = answer_tx.send(value);
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
                    // Ignoré si une tâche est en cours : `spawn_agent_run`
                    // détient alors le verrou pour toute la durée de
                    // `Agent::run`, `try_lock` échouerait de toute façon,
                    // mais autant éviter l'appel et rester explicite sur la
                    // raison de l'ignorance.
                    if !agent_running.load(Ordering::SeqCst)
                        && let Ok(mut history) = agent_history.try_lock()
                    {
                        history.clear();
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
/// domaine/intentions, l'absence de modèle empêche réellement de lancer la
/// boucle agentique.
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
/// que d'empêcher le reste de la boucle agentique de démarrer.
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

fn spawn_agent_run(
    state: Arc<AppState>,
    editor: Arc<dyn LegalActEditorPort>,
    interaction: Arc<dyn UserInteractionPort>,
    agent_observer: Arc<dyn agent::AgentObserver>,
    out_tx: mpsc::UnboundedSender<ServerMessage>,
    agent_running: Arc<AtomicBool>,
    auto_accept: Arc<AtomicBool>,
    agent_history: Arc<tokio::sync::Mutex<Vec<agent::ChatMessage>>>,
    legal_act_id: Option<ID>,
    author_id: ID,
    task: String,
) {
    tokio::spawn(async move {
        let secret_key = state.secret_encryption_key.clone();
        let message = match build_active_language_model(&state.store, secret_key.clone()).await {
            Err(message) => ServerMessage::AgentError { message },
            Ok((model, ai_model_system_prompt)) => {
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
                tools.register(Box::new(AskUserTool::new(interaction.clone())));
                tools.register(Box::new(AskQuestionsTool::new(interaction.clone())));

                if let Some(legal_act_id) = legal_act_id {
                    let intentions: Arc<dyn agent::ports::IntentionPort> = Arc::new(
                        WsIntentions::new(state.store.clone(), legal_act_id, author_id),
                    );
                    tools.register(Box::new(ListIntentionsTool::new(intentions.clone())));
                    tools.register(Box::new(AddIntentionTool::new(intentions.clone())));
                    tools.register(Box::new(RemoveIntentionTool::new(intentions)));
                }

                if allowed_tools.contains("georisques_query")
                    || allowed_tools.contains("icpe_query")
                {
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

                if allowed_tools.contains("legifrance_search")
                    || allowed_tools.contains("legifrance_fetch")
                {
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

                let config = AgentConfig {
                    system_prompt,
                    ..AgentConfig::default()
                };
                let agent = Agent::new(
                    model,
                    tools,
                    interaction,
                    agent_observer,
                    config,
                    auto_accept,
                );

                let mut history = agent_history.lock().await;
                match agent.run(&task, &mut history).await {
                    Ok(_content) => ServerMessage::AgentDone,
                    Err(error) => ServerMessage::AgentError {
                        message: error.to_string(),
                    },
                }
            }
        };
        let _ = out_tx.send(message);
        agent_running.store(false, Ordering::SeqCst);
    });
}
