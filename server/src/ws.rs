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
//!   interactions qu'elle déclenche (`ask_user`, `ask_questions`...).
//!
//! Quand un outil de l'agent modifie le corps de l'acte (ex: `fill_section`),
//! la mise à jour Yrs qui en résulte est diffusée de la même façon qu'une
//! édition utilisateur : tous les clients convergent vers le même document.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use agent::ports::{LegalActEditorPort, UserInteractionPort};
use agent::tools::{
    AskQuestionsTool, AskUserTool, FillSectionTool, GenerateNumberingTool, InsertNodeTool, ReadStructureTool,
    ReadTitleTool, RemoveNodeTool, SetTitleTool, ValidateStructureTool,
};
use agent::{Agent, AgentConfig, ToolRegistry};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use legal_act::BodyNodeId;
use tokio::sync::mpsc;
use yrs::updates::decoder::Decode;
use yrs::{ReadTxn, StateVector, Transact, Update};

use crate::ports::{WsLegalActEditor, WsUserInteraction};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::state::{AppState, Room};

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
    subdivisions numérotées (« Titre I », « Titre II »...) créées via `insert_node`.";

pub async fn ws_handler(
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, room_id, state))
}

async fn handle_socket(socket: WebSocket, room_id: String, state: Arc<AppState>) {
    let room = state.rooms.get_or_create(&room_id);
    let (mut sink, mut stream) = socket.split();

    let initial_update = {
        let body = room.body.lock().await;
        body.doc().transact().encode_state_as_update_v1(&StateVector::default())
    };
    if sink.send(Message::Binary(initial_update.into())).await.is_err() {
        return;
    }

    let mut room_rx = room.updates.subscribe();
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
    let editor: Arc<dyn LegalActEditorPort> = Arc::new(WsLegalActEditor::new(room.clone(), selection.clone()));
    let interaction: Arc<dyn UserInteractionPort> = Arc::new(WsUserInteraction::new(out_tx.clone(), answer_rx));
    let agent_running = Arc::new(AtomicBool::new(false));
    // Persiste pour toute la durée de la connexion (pas seulement une tâche
    // agent) : l'utilisateur peut activer/désactiver l'auto-acceptation
    // indépendamment de `RunAgent`, y compris pendant qu'une tâche est en
    // cours (voir `agent::Agent::dispatch_tool_call`, qui le relit à chaque
    // appel d'outil plutôt qu'à la construction).
    let auto_accept = Arc::new(AtomicBool::new(false));

    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Binary(bytes) => apply_and_broadcast(&room, &bytes).await,
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
                        out_tx.clone(),
                        agent_running.clone(),
                        auto_accept.clone(),
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
                    let parsed = node_id.as_deref().and_then(|raw| raw.parse::<BodyNodeId>().ok());
                    *selection.lock().expect("verrou non empoisonné") = parsed;
                }
                Err(_) => {}
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}

/// Applique une mise à jour Yrs reçue d'un client au document de la salle,
/// puis la rediffuse telle quelle aux autres pairs (une mise à jour Yrs
/// est valide indépendamment de l'état de son destinataire).
async fn apply_and_broadcast(room: &Arc<Room>, bytes: &[u8]) {
    let Ok(update) = Update::decode_v1(bytes) else { return };
    {
        let body = room.body.lock().await;
        if body.doc().transact_mut().apply_update(update).is_err() {
            return;
        }
    }
    let _ = room.updates.send(bytes.to_vec());
}

fn spawn_agent_run(
    state: Arc<AppState>,
    editor: Arc<dyn LegalActEditorPort>,
    interaction: Arc<dyn UserInteractionPort>,
    out_tx: mpsc::UnboundedSender<ServerMessage>,
    agent_running: Arc<AtomicBool>,
    auto_accept: Arc<AtomicBool>,
    task: String,
) {
    tokio::spawn(async move {
        let message = match &state.model {
            None => ServerMessage::AgentError {
                message: "aucun modèle de langage n'est configuré côté serveur".to_string(),
            },
            Some(model) => {
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

                let config = AgentConfig { system_prompt: AGENT_SYSTEM_PROMPT.to_string(), ..AgentConfig::default() };
                let agent = Agent::new(model.clone(), tools, interaction, config, auto_accept);

                match agent.run(&task).await {
                    Ok(content) => ServerMessage::AgentDone { content },
                    Err(error) => ServerMessage::AgentError { message: error.to_string() },
                }
            }
        };
        let _ = out_tx.send(message);
        agent_running.store(false, Ordering::SeqCst);
    });
}
