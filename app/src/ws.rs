//! Client websocket de collaboration : connecte l'éditeur d'acte au salon
//! `/ws/{room_id}` exposé par [`server::ws`] (voir `server/src/ws.rs` et
//! `server/src/protocol.rs`, dont ce module est le pendant côté client).
//!
//! Deux canaux y transitent, exactement comme côté serveur :
//! - des trames **binaires**, qui portent des mises à jour Yrs brutes du
//!   corps de l'acte : celles reçues sont appliquées au [`YrsBody`] partagé
//!   ([`Body::Yrs`]) ; celles produites par une édition locale sont
//!   envoyées au serveur pour être rediffusées aux autres pairs ;
//! - des trames **texte** JSON ([`ClientMessage`]/[`ServerMessage`]), qui
//!   pilotent la boucle agentique (`run_agent`) et relaient les
//!   interactions qu'elle déclenche (`ask_user`, `ask_questions`...) vers
//!   le [`agent::AgentPanel`] affiché par [`legal_act::LegalActEditor`].
//!
//! [`LegalActEditor`] est un composant contrôlé : `body` (ainsi que les
//! signaux `agent_*`) sont possédés par [`RoomHandle`], pas par le
//! composant — c'est ce qui permet à ce module de continuer à les modifier
//! après le montage, à chaque frame reçue du serveur.

use agent::{
    InteractionRequest, InteractionResponse, PanelEntry, PanelQuestion, PanelReasoning,
    PanelToolCall, PanelToolCallStatus,
};
use legal_act::{Body, BodyNodeId, DirectBody, YrsBody};
use leptos::prelude::*;
use web_sys::wasm_bindgen::JsCast;
use web_sys::wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, MessageEvent, WebSocket};
use yrs::updates::decoder::Decode;
// Alias : `yrs::Update` porterait sinon le même nom que le trait
// `leptos::prelude::Update` (fourni par `RwSignal::update`), et l'un
// masquerait l'autre dans la portée de ce module.
use yrs::Update as YrsUpdate;
use yrs::{Doc, Transact};

use crate::protocol::{ClientMessage, InteractionAnswerWire, PresenceUser, ServerMessage};

/// Origine posée sur les transactions Yrs appliquées depuis le réseau, pour
/// que l'observateur de mises à jour locales (voir [`open_socket`]) sache
/// ne pas les retransmettre au serveur (qui les rediffuse déjà lui-même à
/// tous les pairs, y compris à l'émetteur d'origine — voir la documentation
/// de `server::ws::apply_and_broadcast`).
const REMOTE_ORIGIN: &str = "ws-remote";

/// Enveloppe un type non `Send`/`Sync` (poignée JS telle que [`WebSocket`])
/// pour satisfaire les bornes exigées par `Callback`/`RwSignal`/
/// `Doc::observe_update_v1`. Sûr : la cible wasm32 est mono-thread, ces
/// valeurs ne quittent jamais le thread où elles ont été créées.
#[derive(Clone)]
struct WasmSend<T>(T);

unsafe impl<T> Send for WasmSend<T> {}
unsafe impl<T> Sync for WasmSend<T> {}

impl<T> std::ops::Deref for WasmSend<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Distingue la question posée par le serveur, pour reconstruire la valeur
/// JSON attendue par [`ClientMessage::InteractionAnswer`] lors de la
/// réponse (voir [`RoomHandle::respond`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingInteractionKind {
    Ask,
    Confirm,
    Questions,
}

/// Poignée d'un salon de collaboration : signaux à passer tels quels aux
/// props de [`legal_act::LegalActEditor`], plus les méthodes qui envoient
/// des messages de contrôle au serveur.
#[derive(Clone, Copy)]
pub struct RoomHandle {
    /// Corps de l'acte, `Body::Direct` vide tant que la première
    /// synchronisation n'est pas arrivée, `Body::Yrs` ensuite.
    pub body: RwSignal<Body>,
    /// `true` dès que le corps ci-dessus reflète l'état du serveur : la
    /// page hôte doit attendre ce signal avant de monter
    /// [`legal_act::LegalActEditor`] (voir `super::app::PageEditorProjet`).
    pub ready: RwSignal<bool>,
    pub agent_messages: RwSignal<Vec<PanelEntry>>,
    pub agent_pending: RwSignal<bool>,
    pub interaction: RwSignal<Option<InteractionRequest>>,
    /// Index dans `agent_messages` de la réflexion du tour courant,
    /// toujours la dernière entrée poussée tant qu'elle n'est pas figée par
    /// `ServerMessage::AgentStepFinished` (voir [`handle_text_frame`]).
    /// `None` tant qu'aucun fragment de réflexion n'est encore arrivé pour
    /// ce tour.
    open_reasoning: RwSignal<Option<usize>>,
    /// Même principe que `open_reasoning`, pour le message assistant en
    /// cours de réception (fragments de contenu du tour courant).
    open_message: RwSignal<Option<usize>>,
    /// `true` si l'utilisateur a choisi d'accepter automatiquement toutes
    /// les modifications proposées par l'agent, sans confirmation
    /// individuelle (voir [`RoomHandle::set_auto_accept`]).
    pub auto_accept: RwSignal<bool>,
    /// Utilisateurs actuellement connectés à la salle (voir
    /// `server::protocol::ServerMessage::Presence`), pour l'affichage des
    /// pastilles de présence dans l'en-tête de l'éditeur.
    pub connected_users: RwSignal<Vec<PresenceUser>>,
    pending_kind: RwSignal<Option<PendingInteractionKind>>,
    socket: RwSignal<Option<WasmSend<WebSocket>>>,
}

impl RoomHandle {
    /// Active ou désactive l'acceptation automatique des outils de l'agent
    /// qui demanderaient normalement une confirmation, côté serveur pour
    /// cette connexion.
    pub fn set_auto_accept(&self, enabled: bool) {
        self.auto_accept.set(enabled);
        self.send(&ClientMessage::SetAutoAccept { enabled });
    }

    /// Signale au serveur le nœud actuellement ciblé par l'utilisateur dans
    /// l'éditeur (voir [`legal_act::editor::EditorContext::agent_target`]),
    /// pour que l'agent puisse le viser via le mot-clé `"selection"` sans
    /// que l'utilisateur ait à connaître ni transmettre son identifiant
    /// technique.
    pub fn set_selection(&self, node_id: Option<BodyNodeId>) {
        self.send(&ClientMessage::SetSelection {
            node_id: node_id.map(|id| id.to_string()),
        });
    }

    /// Démarre la boucle agentique côté serveur avec `task`, et ajoute
    /// immédiatement le message de l'utilisateur à l'historique affiché.
    pub fn run_agent(&self, task: String) {
        self.agent_messages.update(|m| m.push(PanelEntry::user(task.clone())));
        self.open_reasoning.set(None);
        self.open_message.set(None);
        self.agent_pending.set(true);
        self.send(&ClientMessage::RunAgent { task });
    }

    /// Répond au formulaire d'interaction affiché par [`agent::AgentPanel`].
    /// La forme de la valeur envoyée dépend de la question d'origine
    /// (voir [`server::protocol::ClientMessage::InteractionAnswer`]).
    pub fn respond(&self, response: InteractionResponse) {
        let value = match self.pending_kind.get_untracked() {
            Some(PendingInteractionKind::Ask) => serde_json::Value::String(
                response
                    .answers
                    .into_iter()
                    .next()
                    .map(|a| a.value)
                    .unwrap_or_default(),
            ),
            Some(PendingInteractionKind::Confirm) => {
                let yes = response
                    .answers
                    .first()
                    .is_some_and(|a| a.value.eq_ignore_ascii_case("oui"));
                serde_json::Value::Bool(yes)
            }
            Some(PendingInteractionKind::Questions) | None => {
                let wire: Vec<InteractionAnswerWire> = response
                    .answers
                    .into_iter()
                    .map(|a| InteractionAnswerWire {
                        question_id: a.question_id,
                        value: a.value,
                        unsatisfactory_reason: a.unsatisfactory_reason,
                    })
                    .collect();
                serde_json::to_value(wire).unwrap_or(serde_json::Value::Null)
            }
        };
        self.interaction.set(None);
        self.pending_kind.set(None);
        self.agent_pending.set(true);
        self.send(&ClientMessage::InteractionAnswer { value });
    }

    fn send(&self, message: &ClientMessage) {
        let Some(socket) = self.socket.get_untracked() else {
            return;
        };
        if let Ok(text) = serde_json::to_string(message) {
            let _ = socket.send_with_str(&text);
        }
    }
}

/// Ouvre la connexion vers le salon `room_id` et renvoie sa poignée. Les
/// signaux sont créés immédiatement (utilisables pendant le rendu SSR côté
/// serveur), mais la connexion réseau elle-même n'est établie que côté
/// client : elle est différée dans un [`Effect`], que Leptos n'exécute
/// jamais pendant le rendu serveur.
pub fn connect_room(room_id: impl Into<String>) -> RoomHandle {
    let room_id = room_id.into();
    let handle = RoomHandle {
        body: RwSignal::new(Body::from(DirectBody::new())),
        ready: RwSignal::new(false),
        agent_messages: RwSignal::new(Vec::new()),
        agent_pending: RwSignal::new(false),
        interaction: RwSignal::new(None),
        open_reasoning: RwSignal::new(None),
        open_message: RwSignal::new(None),
        auto_accept: RwSignal::new(false),
        connected_users: RwSignal::new(Vec::new()),
        pending_kind: RwSignal::new(None),
        socket: RwSignal::new(None),
    };

    Effect::new(move |_| {
        open_socket(&room_id, handle);
    });

    handle
}

fn room_ws_url(room_id: &str) -> Option<String> {
    let location = window().location();
    let scheme = if location.protocol().ok()? == "https:" {
        "wss"
    } else {
        "ws"
    };
    let host = location.host().ok()?;
    Some(format!("{scheme}://{host}/editor/{room_id}/ws"))
}

fn open_socket(room_id: &str, handle: RoomHandle) {
    let Some(url) = room_ws_url(room_id) else {
        return;
    };
    let Ok(socket) = WebSocket::new(&url) else {
        return;
    };
    socket.set_binary_type(BinaryType::Arraybuffer);

    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |ev: MessageEvent| {
        on_message(handle, ev);
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    // La connexion vit aussi longtemps que la page : on oublie volontairement
    // la fermeture JS plutôt que de complexifier `RoomHandle` pour la retenir.
    onmessage.forget();

    handle.socket.set(Some(WasmSend(socket)));
}

fn on_message(handle: RoomHandle, ev: MessageEvent) {
    let data = ev.data();
    if let Ok(buf) = data.clone().dyn_into::<web_sys::js_sys::ArrayBuffer>() {
        let bytes = web_sys::js_sys::Uint8Array::new(&buf).to_vec();
        handle_binary_frame(handle, &bytes);
    } else if let Some(text) = data.as_string() {
        handle_text_frame(handle, &text);
    }
}

/// Applique une trame binaire Yrs. La première trame reçue est l'état
/// complet du document (voir `server::ws::handle_socket`) : elle sert à
/// construire le [`YrsBody`] à partir d'un [`Doc`] vide, comme dans
/// `legal_act::crdt::tests::test_open_from_synced_doc`. Les suivantes sont
/// des mises à jour incrémentales, appliquées directement au document
/// existant.
fn handle_binary_frame(handle: RoomHandle, bytes: &[u8]) {
    let Ok(update) = YrsUpdate::decode_v1(bytes) else {
        return;
    };

    if !handle.ready.get_untracked() {
        let doc = Doc::new();
        if doc.transact_mut().apply_update(update).is_err() {
            return;
        }
        let body_map = doc.get_or_insert_map("body");
        let Ok(yrs_body) = YrsBody::open(doc, body_map) else {
            return;
        };

        // Retransmet au serveur chaque mise à jour locale (issue d'une
        // édition de l'utilisateur), en ignorant celles qui proviennent de
        // l'application d'une trame reçue du réseau (origine REMOTE_ORIGIN,
        // posée par la branche `else` ci-dessous).
        if let Some(socket) = handle.socket.get_untracked() {
            let outbound = socket;
            let subscription = yrs_body.doc().clone().observe_update_v1(move |txn, event| {
                let is_remote = txn
                    .origin()
                    .map(|o| o.as_ref() == REMOTE_ORIGIN.as_bytes())
                    .unwrap_or(false);
                if is_remote {
                    return;
                }
                let _ = outbound.send_with_u8_array(&event.update);
            });
            // La Subscription se désabonne à son Drop : on la laisse fuir
            // volontairement pour qu'elle vive aussi longtemps que la page,
            // au même titre que la Closure JS de `open_socket`.
            if let Ok(subscription) = subscription {
                std::mem::forget(subscription);
            }
        }

        handle.body.set(Body::Yrs(yrs_body));
        handle.ready.set(true);
    } else {
        handle.body.update(|b| {
            if let Body::Yrs(yrs_body) = b {
                let _ = yrs_body
                    .doc()
                    .transact_mut_with(REMOTE_ORIGIN)
                    .apply_update(update);
            }
        });
    }
}

/// Traduit un [`ServerMessage`] en mise à jour des signaux exposés par
/// [`RoomHandle`], consommés par [`agent::AgentPanel`].
fn handle_text_frame(handle: RoomHandle, text: &str) {
    let Ok(message) = serde_json::from_str::<ServerMessage>(text) else {
        return;
    };
    match message {
        ServerMessage::AgentDone => {
            handle.agent_pending.set(false);
        }
        ServerMessage::AgentError { message } => {
            handle.open_reasoning.set(None);
            handle.open_message.set(None);
            handle
                .agent_messages
                .update(|m| m.push(PanelEntry::assistant(format!("Erreur : {message}"))));
            handle.agent_pending.set(false);
        }
        ServerMessage::AgentReasoningDelta { delta } => {
            append_reasoning_delta(handle, delta);
        }
        ServerMessage::AgentContentDelta { delta } => {
            append_content_delta(handle, delta);
        }
        ServerMessage::AgentStepFinished => {
            if let Some(idx) = handle.open_reasoning.get_untracked() {
                handle.agent_messages.update(|entries| {
                    if let Some(PanelEntry::Reasoning(reasoning)) = entries.get_mut(idx) {
                        reasoning.done = true;
                    }
                });
            }
            handle.open_reasoning.set(None);
            handle.open_message.set(None);
        }
        ServerMessage::AgentToolCallStarted { id, name, arguments } => {
            let arguments = serde_json::to_string_pretty(&arguments)
                .unwrap_or_else(|_| arguments.to_string());
            handle.agent_messages.update(|entries| {
                entries.push(PanelEntry::ToolCall(PanelToolCall {
                    id,
                    name,
                    arguments,
                    status: PanelToolCallStatus::Running,
                }));
            });
        }
        ServerMessage::AgentToolCallFinished { id, ok, output } => {
            let status = if ok {
                PanelToolCallStatus::Done { output }
            } else {
                PanelToolCallStatus::Error { message: output }
            };
            handle.agent_messages.update(|entries| {
                set_tool_call_status(entries, &id, status);
            });
        }
        ServerMessage::InteractionAsk { question } => {
            handle.agent_pending.set(false);
            handle.pending_kind.set(Some(PendingInteractionKind::Ask));
            handle.interaction.set(Some(InteractionRequest {
                prompt: question,
                questions: vec![PanelQuestion {
                    id: "reponse".to_string(),
                    label: "Votre réponse".to_string(),
                    options: None,
                }],
            }));
        }
        ServerMessage::InteractionConfirm { message } => {
            handle.agent_pending.set(false);
            handle
                .pending_kind
                .set(Some(PendingInteractionKind::Confirm));
            handle.interaction.set(Some(InteractionRequest {
                prompt: message,
                questions: vec![PanelQuestion {
                    id: "confirmation".to_string(),
                    label: "Confirmez-vous ?".to_string(),
                    options: Some(vec!["Oui".to_string(), "Non".to_string()]),
                }],
            }));
        }
        ServerMessage::InteractionQuestions { prompt, questions } => {
            handle.agent_pending.set(false);
            handle
                .pending_kind
                .set(Some(PendingInteractionKind::Questions));
            handle.interaction.set(Some(InteractionRequest {
                prompt,
                questions: questions
                    .into_iter()
                    .map(|q| PanelQuestion {
                        id: q.id,
                        label: q.label,
                        options: q.options,
                    })
                    .collect(),
            }));
        }
        ServerMessage::Presence { users } => {
            handle.connected_users.set(users);
        }
    }
}

/// Ajoute `delta` à la réflexion du tour courant, ouvrant une nouvelle
/// entrée [`PanelEntry::Reasoning`] s'il n'y en a pas déjà une pour ce tour
/// (voir `RoomHandle::open_reasoning`, remis à `None` par
/// `ServerMessage::AgentStepFinished`).
fn append_reasoning_delta(handle: RoomHandle, delta: String) {
    if let Some(idx) = handle.open_reasoning.get_untracked() {
        handle.agent_messages.update(|entries| {
            if let Some(PanelEntry::Reasoning(reasoning)) = entries.get_mut(idx) {
                reasoning.content.push_str(&delta);
            }
        });
        return;
    }
    let mut new_idx = 0;
    handle.agent_messages.update(|entries| {
        entries.push(PanelEntry::Reasoning(PanelReasoning {
            content: delta,
            done: false,
        }));
        new_idx = entries.len() - 1;
    });
    handle.open_reasoning.set(Some(new_idx));
}

/// Même principe que [`append_reasoning_delta`], pour le message assistant
/// (contenu final ou narration) du tour courant.
fn append_content_delta(handle: RoomHandle, delta: String) {
    if let Some(idx) = handle.open_message.get_untracked() {
        handle.agent_messages.update(|entries| {
            if let Some(PanelEntry::Message(message)) = entries.get_mut(idx) {
                message.content.push_str(&delta);
            }
        });
        return;
    }
    let mut new_idx = 0;
    handle.agent_messages.update(|entries| {
        entries.push(PanelEntry::assistant(delta));
        new_idx = entries.len() - 1;
    });
    handle.open_message.set(Some(new_idx));
}

/// Retrouve la trace d'appel d'outil `id` (la plus récente : les
/// identifiants d'appel sont générés par le modèle et supposés uniques par
/// tour, mais rien n'empêche un fournisseur de les réutiliser d'un tour à
/// l'autre) et met à jour son statut.
fn set_tool_call_status(entries: &mut [PanelEntry], id: &str, status: PanelToolCallStatus) {
    for entry in entries.iter_mut().rev() {
        if let PanelEntry::ToolCall(call) = entry
            && call.id == id
        {
            call.status = status;
            return;
        }
    }
}
