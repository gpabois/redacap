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
//!   le [`agent::AgentPanel`] affiché par [`legal_act::LegalActEditor`] ;
//!   elles portent aussi (voir [`ClientMessage::ReviewUpdate`]/
//!   [`ServerMessage::ReviewUpdate`]) les mises à jour Yrs (base64) du
//!   second document Yrs des commentaires/notes de travail
//!   ([`legal_act::Review`]), pour ne pas dupliquer le multiplexage des
//!   trames binaires pour un document dont le volume est bien plus faible
//!   (voir [`handle_review_update`]).
//!
//! [`LegalActEditor`] est un composant contrôlé : `body` (ainsi que les
//! signaux `agent_*`) sont possédés par [`RoomHandle`], pas par le
//! composant — c'est ce qui permet à ce module de continuer à les modifier
//! après le montage, à chaque frame reçue du serveur.

use agent::{
    AgentSessionHistory, AgentSessionSummary, DocumentRequest, DocumentUpload, InteractionRequest,
    InteractionResponse, PanelEntry, PanelQuestion, PanelReasoning, PanelToolCall,
    PanelToolCallStatus, SupervisorContextEntry, SupervisorContextToolCall,
};
use base64::Engine;
use legal_act::{Body, BodyNodeId, DirectBody, Review, YrsBody, YrsReview};
use leptos::prelude::*;
use shared::broadcast::{DocumentsChangedEvent, MetadataChangedEvent};
use web_sys::wasm_bindgen::JsCast;
use web_sys::wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, MessageEvent, WebSocket};
use yrs::updates::decoder::Decode;
// Alias : `yrs::Update` porterait sinon le même nom que le trait
// `leptos::prelude::Update` (fourni par `RwSignal::update`), et l'un
// masquerait l'autre dans la portée de ce module.
use yrs::Update as YrsUpdate;
use yrs::{Doc, Transact};

use crate::protocol::{
    AgentSessionEntryWire, AgentSessionWire, ClientMessage, DocumentUploadWire,
    InteractionAnswerWire, PresenceUser, ServerMessage, SupervisorContextEntryWire,
};

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
    /// Commentaires et notes de travail du projet (voir [`legal_act::Review`]),
    /// `Review::Direct` vide tant que l'état initial n'est pas arrivé
    /// (voir [`ServerMessage::ReviewUpdate`]), `Review::Yrs` ensuite —
    /// pendant de [`Self::body`] pour ce second document Yrs.
    pub reviews: RwSignal<Review>,
    /// `true` dès que le corps ci-dessus reflète l'état du serveur : la
    /// page hôte doit attendre ce signal avant de monter
    /// [`legal_act::LegalActEditor`] (voir `super::app::PageEditorProjet`).
    pub ready: RwSignal<bool>,
    pub agent_messages: RwSignal<Vec<PanelEntry>>,
    pub agent_pending: RwSignal<bool>,
    /// Dernière tâche envoyée à l'agent via [`Self::run_agent`] (voir
    /// [`Self::restart_agent`]), `None` tant qu'aucune tâche n'a encore été
    /// envoyée depuis l'ouverture de cette page.
    last_task: RwSignal<Option<String>>,
    pub interaction: RwSignal<Option<InteractionRequest>>,
    /// Requête de document affichée par [`agent::AgentPanel`] lorsque
    /// l'agent attend un upload (outil `request_document`), pendant de
    /// `interaction` pour ce type d'interaction (voir [`Self::respond_document`]).
    pub document_request: RwSignal<Option<DocumentRequest>>,
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
    /// Sessions de conversation passées de l'utilisateur courant pour cette
    /// salle (voir [`Self::list_agent_sessions`]), affichées par
    /// [`agent::AgentPanel`]'s `sessions`.
    pub agent_sessions: RwSignal<Vec<AgentSessionSummary>>,
    /// Transcript d'une session passée en cours de consultation (voir
    /// [`Self::open_agent_session`]), affiché par [`agent::AgentPanel`]'s
    /// `session_history` en recouvrement de `agent_messages` sans jamais le
    /// modifier.
    pub agent_session_history: RwSignal<Option<AgentSessionHistory>>,
    /// Contexte brut du Superviseur en cours de consultation (voir
    /// [`Self::view_supervisor_context`]), affiché par [`agent::AgentPanel`]'s
    /// `supervisor_context` en recouvrement de `agent_messages`, sans jamais
    /// le modifier.
    pub supervisor_context: RwSignal<Option<Vec<SupervisorContextEntry>>>,
    /// Incrémenté à chaque [`ServerMessage::MetadataChanged`] reçu (écriture
    /// ou suppression d'une métadonnée du projet par l'agent ou par un autre
    /// utilisateur), pour que `crate::pages::project_metadata::ProjectMetadataPanel`
    /// puisse s'en servir de clé de rechargement sans dépendre de l'égalité
    /// de [`Self::metadata_last_change`] (qui ne changerait pas si la même
    /// clé était réécrite avec la même valeur).
    pub metadata_version: RwSignal<u32>,
    /// Dernier changement de métadonnée reçu (voir [`Self::metadata_version`]),
    /// affiché par `ProjectMetadataPanel` comme un avis ponctuel de ce qui
    /// vient de changer et par qui.
    pub metadata_last_change: RwSignal<Option<MetadataChangedEvent>>,
    /// Incrémenté à chaque [`ServerMessage::DocumentsChanged`] reçu (ajout ou
    /// suppression d'un document du projet par l'agent ou par un autre
    /// utilisateur), pendant de [`Self::metadata_version`] pour
    /// `crate::pages::project_documents::ProjectFilesPanel`.
    pub files_version: RwSignal<u32>,
    /// Dernier changement de document reçu (voir [`Self::files_version`]),
    /// affiché par `ProjectFilesPanel` comme un avis ponctuel de ce qui vient
    /// de changer et par qui.
    pub files_last_change: RwSignal<Option<DocumentsChangedEvent>>,
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
        self.last_task.set(Some(task.clone()));
        self.agent_messages
            .update(|m| m.push(PanelEntry::user(task.clone())));
        self.open_reasoning.set(None);
        self.open_message.set(None);
        self.agent_pending.set(true);
        self.send(&ClientMessage::RunAgent { task });
    }

    /// Demande au serveur d'arrêter immédiatement la tâche agent en cours
    /// (voir `server::editor::protocol::ClientMessage::StopAgent`) : sans
    /// effet si aucune tâche n'est en cours. Ne lève pas immédiatement
    /// `agent_pending` — c'est [`ServerMessage::AgentStopped`], renvoyé une
    /// fois l'arrêt effectif côté serveur, qui s'en charge.
    pub fn stop_agent(&self) {
        self.send(&ClientMessage::StopAgent);
    }

    /// Relance la dernière tâche envoyée à l'agent (voir [`Self::last_task`]),
    /// par exemple après un arrêt volontaire ou une erreur : sans effet si
    /// aucune tâche n'a encore été envoyée sur cette page.
    pub fn restart_agent(&self) {
        if let Some(task) = self.last_task.get_untracked() {
            self.run_agent(task);
        }
    }

    /// Efface l'historique affiché et la conversation tenue côté serveur
    /// (voir [`server::protocol::ClientMessage::ClearHistory`]) : sans effet
    /// tant qu'une tâche agent est en cours (voir `agent_pending`), pour ne
    /// pas effacer sous les pieds d'une réponse en train d'arriver.
    pub fn clear_history(&self) {
        if self.agent_pending.get_untracked() {
            return;
        }
        self.agent_messages.set(Vec::new());
        self.open_reasoning.set(None);
        self.open_message.set(None);
        self.interaction.set(None);
        self.pending_kind.set(None);
        self.send(&ClientMessage::ClearHistory);
    }

    /// Demande la liste des sessions de conversation passées de l'utilisateur
    /// courant pour cette salle (voir [`ServerMessage::AgentSessions`], qui
    /// alimente [`Self::agent_sessions`]).
    pub fn list_agent_sessions(&self) {
        self.send(&ClientMessage::ListAgentSessions);
    }

    /// Demande la reconstruction en lecture seule du transcript de la session
    /// passée `session_id` (voir [`ServerMessage::AgentSessionHistory`], qui
    /// alimente [`Self::agent_session_history`]) : sans effet sur la
    /// conversation en cours (`agent_messages`).
    pub fn open_agent_session(&self, session_id: String) {
        self.send(&ClientMessage::GetAgentSessionHistory { session_id });
    }

    /// Ferme la consultation d'une session passée : restaure l'affichage de
    /// la conversation en cours, inchangée entre-temps.
    pub fn close_agent_session_history(&self) {
        self.agent_session_history.set(None);
    }

    /// Demande le contexte brut (historique `agent::ChatMessage`, système
    /// compris) effectivement envoyé au modèle par le frame Superviseur du
    /// run le plus récent de cette salle (voir
    /// [`ServerMessage::SupervisorContext`], qui alimente
    /// [`Self::supervisor_context`]).
    pub fn view_supervisor_context(&self) {
        self.send(&ClientMessage::GetSupervisorContext);
    }

    /// Ferme la consultation du contexte du Superviseur : restaure
    /// l'affichage de la conversation en cours, inchangée entre-temps.
    pub fn close_supervisor_context(&self) {
        self.supervisor_context.set(None);
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

    /// Répond au sélecteur de document affiché par [`agent::AgentPanel`]
    /// lorsque l'agent attend un upload (voir [`Self::document_request`]).
    pub fn respond_document(&self, upload: DocumentUpload) {
        let wire = DocumentUploadWire {
            file_name: upload.file_name,
            mime_type: upload.mime_type,
            content_base64: upload.content_base64,
        };
        let value = serde_json::to_value(wire).unwrap_or(serde_json::Value::Null);
        self.document_request.set(None);
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
        reviews: RwSignal::new(Review::direct()),
        ready: RwSignal::new(false),
        agent_messages: RwSignal::new(Vec::new()),
        agent_pending: RwSignal::new(false),
        last_task: RwSignal::new(None),
        interaction: RwSignal::new(None),
        document_request: RwSignal::new(None),
        open_reasoning: RwSignal::new(None),
        open_message: RwSignal::new(None),
        auto_accept: RwSignal::new(false),
        connected_users: RwSignal::new(Vec::new()),
        agent_sessions: RwSignal::new(Vec::new()),
        agent_session_history: RwSignal::new(None),
        supervisor_context: RwSignal::new(None),
        metadata_version: RwSignal::new(0),
        metadata_last_change: RwSignal::new(None),
        files_version: RwSignal::new(0),
        files_last_change: RwSignal::new(None),
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
                .update(|m| m.push(PanelEntry::error(String::new(), message)));
            handle.agent_pending.set(false);
        }
        ServerMessage::AgentStopped => {
            handle.open_reasoning.set(None);
            handle.open_message.set(None);
            handle
                .agent_messages
                .update(|m| m.push(PanelEntry::stopped("Tâche interrompue par l'utilisateur.")));
            handle.agent_pending.set(false);
        }
        ServerMessage::AgentReasoningDelta { agent_label, delta } => {
            // Positionné défensivement à chaque fragment reçu, pas seulement
            // au démarrage local de la tâche (voir `RoomHandle::run`) : cette
            // trame peut provenir d'une tâche démarrée depuis un autre onglet
            // ou par un autre collaborateur de la salle (voir
            // `server::editor::state::EditorRoom::agent_events`), que cette
            // connexion n'a donc jamais vu démarrer.
            handle.agent_pending.set(true);
            append_reasoning_delta(handle, agent_label, delta);
        }
        ServerMessage::AgentContentDelta { agent_label, delta } => {
            handle.agent_pending.set(true);
            append_content_delta(handle, agent_label, delta);
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
        ServerMessage::AgentToolCallStarted {
            agent_label,
            id,
            name,
            arguments,
        } => {
            let arguments =
                serde_json::to_string_pretty(&arguments).unwrap_or_else(|_| arguments.to_string());
            handle.agent_pending.set(true);
            handle.agent_messages.update(|entries| {
                entries.push(PanelEntry::ToolCall(PanelToolCall {
                    id,
                    agent_label,
                    name,
                    arguments,
                    status: PanelToolCallStatus::Running,
                }));
            });
        }
        ServerMessage::AgentToolCallFinished { id, ok, output, .. } => {
            let status = if ok {
                PanelToolCallStatus::Done { output }
            } else {
                PanelToolCallStatus::Error { message: output }
            };
            handle.agent_messages.update(|entries| {
                set_tool_call_status(entries, &id, status);
            });
        }
        ServerMessage::InteractionAsk {
            agent_label,
            question,
        } => {
            handle.agent_pending.set(false);
            handle.pending_kind.set(Some(PendingInteractionKind::Ask));
            handle.interaction.set(Some(InteractionRequest {
                agent_label,
                prompt: question,
                questions: vec![PanelQuestion {
                    id: "reponse".to_string(),
                    label: "Votre réponse".to_string(),
                    options: None,
                }],
            }));
        }
        ServerMessage::InteractionConfirm {
            agent_label,
            message,
        } => {
            handle.agent_pending.set(false);
            handle
                .pending_kind
                .set(Some(PendingInteractionKind::Confirm));
            handle.interaction.set(Some(InteractionRequest {
                agent_label,
                prompt: message,
                questions: vec![PanelQuestion {
                    id: "confirmation".to_string(),
                    label: "Confirmez-vous ?".to_string(),
                    options: Some(vec!["Oui".to_string(), "Non".to_string()]),
                }],
            }));
        }
        ServerMessage::InteractionQuestions {
            agent_label,
            prompt,
            questions,
        } => {
            handle.agent_pending.set(false);
            handle
                .pending_kind
                .set(Some(PendingInteractionKind::Questions));
            handle.interaction.set(Some(InteractionRequest {
                agent_label,
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
        ServerMessage::InteractionRequestDocument {
            agent_label,
            prompt,
            accepted_mime_types,
        } => {
            handle.agent_pending.set(false);
            handle.document_request.set(Some(DocumentRequest {
                agent_label,
                prompt,
                accepted_mime_types,
            }));
        }
        ServerMessage::Presence { users } => {
            handle.connected_users.set(users);
        }
        ServerMessage::ReviewUpdate { update } => {
            handle_review_update(handle, &update);
        }
        ServerMessage::AgentSessions { sessions } => {
            let sessions = sessions.into_iter().map(session_wire_to_summary).collect();
            handle.agent_sessions.set(sessions);
        }
        ServerMessage::AgentSessionHistory {
            session_id,
            entries,
        } => {
            let entries = entries
                .into_iter()
                .enumerate()
                .map(|(index, entry)| session_entry_to_panel_entry(index, entry))
                .collect();
            handle.agent_session_history.set(Some(AgentSessionHistory {
                session_id,
                entries,
            }));
        }
        ServerMessage::SupervisorContext { entries } => {
            let entries = entries.into_iter().map(context_wire_to_entry).collect();
            handle.supervisor_context.set(Some(entries));
        }
        ServerMessage::AgentActiveSession { entries } => {
            // Ne remplace `agent_messages` que s'il est encore vide : ce
            // message n'arrive qu'une fois, juste après l'ouverture de la
            // connexion (voir `server::editor::ws::restore_active_session`),
            // avant qu'aucun échange de cette page n'ait pu y être ajouté.
            if handle.agent_messages.get_untracked().is_empty() {
                let restored = entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, entry)| session_entry_to_panel_entry(index, entry))
                    .collect();
                handle.agent_messages.set(restored);
            }
        }
        ServerMessage::AgentRunInProgress => {
            handle.agent_pending.set(true);
        }
        ServerMessage::MetadataChanged(event) => {
            handle.metadata_last_change.set(Some(event));
            handle.metadata_version.update(|v| *v = v.wrapping_add(1));
        }
        ServerMessage::DocumentsChanged(event) => {
            handle.files_last_change.set(Some(event));
            handle.files_version.update(|v| *v = v.wrapping_add(1));
        }
    }
}

/// Met en forme l'horodatage RFC 3339 d'une session en libellé lisible
/// (`AAAA-MM-JJ HH:MM`), sans dépendre d'une bibliothèque de dates côté
/// wasm : une troncature de la représentation ISO 8601 suffit à cet usage
/// d'affichage, la page hôte n'a jamais besoin de la reparser.
fn format_session_timestamp(rfc3339: &str) -> String {
    rfc3339
        .get(0..16)
        .map(|prefix| prefix.replace('T', " "))
        .unwrap_or_else(|| rfc3339.to_string())
}

fn session_wire_to_summary(wire: AgentSessionWire) -> AgentSessionSummary {
    let mut label = format_session_timestamp(&wire.created_at);
    if wire.status == "active" {
        label.push_str(" — en cours");
    } else if let Some(archived_at) = &wire.archived_at {
        label.push_str(&format!(
            " — archivée le {}",
            format_session_timestamp(archived_at)
        ));
    }
    AgentSessionSummary {
        id: wire.id,
        label,
        preview: wire.preview,
    }
}

/// Convertit une entrée de transcript reconstruit en [`PanelEntry`] affichable
/// (voir `agent::render_panel_entry`, invoqué par `agent::AgentPanel` pour son
/// recouvrement en lecture seule) : `index` sert uniquement à donner un
/// identifiant stable à une trace d'appel d'outil, jamais réutilisé pour
/// répondre au serveur (contrairement à `AgentToolCallStarted::id`, propre
/// aux appels de la conversation en cours).
fn session_entry_to_panel_entry(index: usize, entry: AgentSessionEntryWire) -> PanelEntry {
    match entry {
        AgentSessionEntryWire::User { content } => PanelEntry::user(content),
        AgentSessionEntryWire::Assistant { content } => PanelEntry::assistant(content),
        AgentSessionEntryWire::ToolCall {
            name,
            arguments,
            output,
        } => {
            let arguments =
                serde_json::to_string_pretty(&arguments).unwrap_or_else(|_| arguments.to_string());
            PanelEntry::ToolCall(PanelToolCall {
                id: format!("session-entry-{index}"),
                agent_label: String::new(),
                name,
                arguments,
                status: PanelToolCallStatus::Done { output },
            })
        }
    }
}

/// Convertit un message brut de contexte reçu du serveur en
/// [`SupervisorContextEntry`] affichable (voir `agent::render_panel_entry`'s
/// pendant `render_supervisor_context_entry`, invoqué par `agent::AgentPanel`
/// pour son recouvrement en lecture seule).
fn context_wire_to_entry(entry: SupervisorContextEntryWire) -> SupervisorContextEntry {
    match entry {
        SupervisorContextEntryWire::System { content } => {
            SupervisorContextEntry::System { content }
        }
        SupervisorContextEntryWire::User { content } => SupervisorContextEntry::User { content },
        SupervisorContextEntryWire::Assistant {
            content,
            tool_calls,
        } => SupervisorContextEntry::Assistant {
            content,
            tool_calls: tool_calls
                .into_iter()
                .map(|call| SupervisorContextToolCall {
                    id: call.id,
                    name: call.name,
                    arguments: call.arguments,
                })
                .collect(),
        },
        SupervisorContextEntryWire::ToolResult {
            tool_call_id,
            content,
        } => SupervisorContextEntry::ToolResult {
            tool_call_id,
            content,
        },
    }
}

/// Applique une mise à jour Yrs (base64) du document de commentaires/notes
/// de travail. Pendant de [`handle_binary_frame`] pour ce second document,
/// relayé sur le canal texte plutôt que binaire (voir
/// `server::editor::protocol::ClientMessage::ReviewUpdate`) : la première
/// mise à jour reçue est l'état complet du document (construit à partir
/// d'un [`Doc`] vide) ; les suivantes sont des mises à jour incrémentales,
/// appliquées directement au document existant.
fn handle_review_update(handle: RoomHandle, update_b64: &str) {
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(update_b64) else {
        return;
    };
    let Ok(update) = YrsUpdate::decode_v1(&bytes) else {
        return;
    };

    let already_yrs = handle
        .reviews
        .with_untracked(|r| matches!(r, Review::Yrs(_)));
    if !already_yrs {
        let doc = Doc::new();
        if doc.transact_mut().apply_update(update).is_err() {
            return;
        }
        let review_map = doc.get_or_insert_map("review");
        let Ok(yrs_review) = YrsReview::open(doc, review_map) else {
            return;
        };

        // Retransmet au serveur chaque mise à jour locale (nouveau
        // commentaire, résolution, suppression...), en ignorant celles
        // provenant de l'application d'une trame reçue du réseau (même
        // idiome que `handle_binary_frame` pour le corps de l'acte).
        if let Some(socket) = handle.socket.get_untracked() {
            let outbound = socket;
            let subscription = yrs_review
                .doc()
                .clone()
                .observe_update_v1(move |txn, event| {
                    let is_remote = txn
                        .origin()
                        .map(|o| o.as_ref() == REMOTE_ORIGIN.as_bytes())
                        .unwrap_or(false);
                    if is_remote {
                        return;
                    }
                    let message = ClientMessage::ReviewUpdate {
                        update: base64::engine::general_purpose::STANDARD.encode(&event.update),
                    };
                    if let Ok(text) = serde_json::to_string(&message) {
                        let _ = outbound.send_with_str(&text);
                    }
                });
            if let Ok(subscription) = subscription {
                std::mem::forget(subscription);
            }
        }

        handle.reviews.set(Review::Yrs(yrs_review));
    } else {
        handle.reviews.update(|r| {
            if let Review::Yrs(yrs_review) = r {
                let _ = yrs_review
                    .doc()
                    .transact_mut_with(REMOTE_ORIGIN)
                    .apply_update(update);
            }
        });
    }
}

/// Ajoute `delta` à la réflexion du tour courant, ouvrant une nouvelle
/// entrée [`PanelEntry::Reasoning`] s'il n'y en a pas déjà une pour ce tour
/// (voir `RoomHandle::open_reasoning`, remis à `None` par
/// `ServerMessage::AgentStepFinished`).
fn append_reasoning_delta(handle: RoomHandle, agent_label: String, delta: String) {
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
            agent_label,
            content: delta,
            done: false,
        }));
        new_idx = entries.len() - 1;
    });
    handle.open_reasoning.set(Some(new_idx));
}

/// Même principe que [`append_reasoning_delta`], pour le message assistant
/// (contenu final ou narration) du tour courant.
fn append_content_delta(handle: RoomHandle, agent_label: String, delta: String) {
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
        entries.push(PanelEntry::assistant_from(agent_label, delta));
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
