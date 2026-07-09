//! Panneau de discussion avec l'agent IA, sous forme de composant Leptos
//! autonome : il ignore tout de la boucle agentique ou du modèle de
//! langage utilisés, et ne fait qu'afficher un historique de messages et
//! relayer la saisie de l'utilisateur via `on_send`. La page hôte reste
//! responsable de l'appel réel à l'agent (typiquement via une fonction
//! serveur Leptos) et de la mise à jour de `messages`/`pending` en retour.

use dsfr::{Alert, Badge, ResizeHandle, Severity};
use leptos::prelude::*;
use leptos::*;
use pulldown_cmark::{Event, Options, Parser};
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

/// Largeur initiale du panneau de formulaire d'interaction, en pixels,
/// lorsqu'il est affiché à côté de l'historique de chat.
const INTERACTION_PANEL_DEFAULT_WIDTH: f64 = 280.0;
/// Bornes de largeur autorisées lors du redimensionnement par glissement
/// (voir [`ResizeHandle`]).
const INTERACTION_PANEL_MIN_WIDTH: f64 = 200.0;
const INTERACTION_PANEL_MAX_WIDTH: f64 = 480.0;

/// Libellé humain d'un appel d'outil, tel qu'affiché dans l'historique du
/// panneau : traduit l'identifiant technique (`Tool::name()`, snake_case)
/// en une phrase compréhensible pour l'inspecteur, qui n'a pas à connaître
/// le nom interne des outils de l'agent. Couvre tous les outils enregistrés
/// côté serveur (voir `agent::tools`) ; un nom non répertorié (futur outil)
/// retombe sur son identifiant technique brut plutôt que de planter
/// l'affichage.
fn tool_display_name(name: &str) -> &str {
    match name {
        "read_structure" => "Lecture de la structure de l'acte",
        "read_title" => "Lecture du titre de l'acte",
        "set_title" => "Modification du titre de l'acte",
        "fill_section" => "Rédaction d'une section",
        "insert_node" => "Insertion d'un élément dans l'acte",
        "remove_node" => "Suppression d'un élément de l'acte",
        "generate_numbering" => "Recalcul de la numérotation",
        "validate_structure" => "Vérification de la structure de l'acte",
        "read_metadata" => "Lecture d'une métadonnée",
        "write_metadata" => "Mise à jour d'une métadonnée",
        "search_metadata" => "Recherche parmi les métadonnées",
        "list_intentions" => "Liste des intentions du projet",
        "add_intention" => "Association d'une intention au projet",
        "remove_intention" => "Retrait d'une intention du projet",
        "ask_user" => "Question posée à l'utilisateur",
        "ask_questions" => "Formulaire présenté à l'utilisateur",
        "request_document" => "Demande d'un document à l'utilisateur",
        "read_document" => "Lecture d'un document externe",
        "fetch_document_by_url" => "Récupération d'un document par URL",
        "search_documents" => "Recherche parmi les documents fournis",
        "legifrance_search" => "Recherche Légifrance",
        "legifrance_fetch" => "Lecture d'un texte Légifrance",
        "georisques_query" => "Interrogation GéoRisques",
        "icpe_query" => "Interrogation de la base ICPE",
        "spawn_expert" => "Sous-tâche confiée au Superviseur",
        _ => name,
    }
}

/// Tronque `s` à `max` caractères (approximatif, sur les `char`) pour
/// l'affichage en aperçu, avec une ellipse si nécessaire.
fn truncate_for_summary(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Résumé lisible du ou des arguments les plus significatifs d'un appel
/// d'outil, affiché à côté de son libellé dans l'en-tête replié : permet de
/// distinguer d'un coup d'oeil, par exemple, deux appels à `fill_section`
/// sans déplier chacun d'eux. Le JSON complet reste consultable au dépli
/// (voir `render_panel_entry`). Renvoie `None` si l'outil n'a pas
/// d'argument significatif à mettre en avant (ex. `read_structure`) ou si
/// les arguments ne sont pas l'objet JSON attendu.
fn tool_call_summary(name: &str, arguments: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let object = value.as_object()?;
    let candidates: &[&str] = match name {
        "fill_section" => &["section_id"],
        "insert_node" => &["kind", "parent_id"],
        "remove_node" => &["node_id"],
        "set_title" => &["title"],
        "read_metadata" | "write_metadata" => &["key"],
        "search_metadata" => &["query"],
        "add_intention" | "remove_intention" => &["intention_id"],
        "legifrance_search" => &["query"],
        "legifrance_fetch" => &["textId", "textCid", "id"],
        "request_document" => &["prompt"],
        "ask_user" => &["question"],
        "read_document" => &["document_id"],
        "fetch_document_by_url" => &["url"],
        "search_documents" => &["query"],
        "georisques_query" => &["code_insee", "latlon"],
        "icpe_query" => &["nom_etablissement", "code_insee"],
        "spawn_expert" => &["task"],
        _ => return None,
    };
    candidates
        .iter()
        .find_map(|key| object.get(*key).and_then(|v| v.as_str()))
        .filter(|s| !s.is_empty())
        .map(|s| truncate_for_summary(s, 80))
}

/// Convertit un texte Markdown (réponse de l'agent) en HTML affichable.
///
/// Les balises HTML brutes présentes dans la source sont supprimées : le
/// texte de l'agent peut refléter du contenu utilisateur, donc on ne peut
/// pas le faire confiance pour injecter du HTML tel quel (XSS).
fn render_markdown(source: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);
    html_output
}

/// Rôle de l'émetteur d'un message affiché dans le panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelRole {
    User,
    Assistant,
}

/// Message affiché dans l'historique du panneau.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PanelMessage {
    pub role: PanelRole,
    pub content: String,
    /// Libellé du frame à l'origine de ce message (voir
    /// [`PanelReasoning::agent_label`]) : vide pour les messages utilisateur,
    /// ou si l'application hôte ne distingue pas plusieurs agents.
    pub agent_label: String,
}

impl PanelMessage {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: PanelRole::User,
            content: content.into(),
            agent_label: String::new(),
        }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: PanelRole::Assistant,
            content: content.into(),
            agent_label: String::new(),
        }
    }

    /// Comme [`Self::assistant`], en précisant le frame (Superviseur ou
    /// expert délégué) à l'origine du message.
    #[must_use]
    pub fn assistant_from(agent_label: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: PanelRole::Assistant,
            content: content.into(),
            agent_label: agent_label.into(),
        }
    }
}

/// Réflexion (chaîne de raisonnement) du modèle, affichée séparément de sa
/// réponse : `done` distingue une réflexion achevée d'une réflexion encore
/// en cours de réception (voir [`AgentPanel`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PanelReasoning {
    /// Libellé du frame à l'origine de cette réflexion (`"Superviseur"` ou
    /// le nom d'un expert délégué, voir `agent::orchestration::AgentFrame`) ;
    /// vide si l'application hôte ne distingue pas plusieurs agents.
    pub agent_label: String,
    pub content: String,
    pub done: bool,
}

/// État d'un appel d'outil tracé dans l'historique (voir [`PanelToolCall`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PanelToolCallStatus {
    /// L'outil est en cours d'exécution (éventuellement en attente de
    /// confirmation de l'utilisateur, voir [`InteractionRequest`]).
    Running,
    Done {
        output: String,
    },
    Error {
        message: String,
    },
}

/// Trace d'un appel d'outil déclenché par l'agent, affichée dans
/// l'historique au même titre qu'un message : nom, arguments et résultat
/// (une fois disponible).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PanelToolCall {
    pub id: String,
    /// Libellé du frame à l'origine de cet appel (voir [`PanelReasoning::agent_label`]).
    pub agent_label: String,
    pub name: String,
    /// Arguments de l'appel, tels que sérialisés (JSON) par l'appelant.
    pub arguments: String,
    pub status: PanelToolCallStatus,
}

/// Distingue un échec réel d'un arrêt volontaire de l'utilisateur dans une
/// [`PanelError`] : les deux interrompent la tâche en cours de la même façon
/// côté panneau, mais méritent un ton et une couleur différents (voir
/// [`render_panel_entry`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelErrorKind {
    /// La boucle agentique s'est arrêtée d'elle-même (erreur de modèle,
    /// d'outil, de configuration...).
    Failure,
    /// L'utilisateur a explicitement demandé l'arrêt de la tâche (voir
    /// [`AgentPanel`]'s `on_stop`).
    Stopped,
}

/// Échec (ou arrêt volontaire) de la tâche agent, affiché dans l'historique
/// comme une entrée à part entière plutôt que noyé dans un message assistant
/// ordinaire (voir [`render_panel_entry`]) : c'est ce qui permet de le
/// distinguer visuellement d'une réponse normale de l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PanelError {
    /// Libellé du frame à l'origine de l'échec (voir [`PanelReasoning::agent_label`]),
    /// vide si l'application hôte ne le précise pas (ex. arrêt volontaire).
    pub agent_label: String,
    pub message: String,
    pub kind: PanelErrorKind,
}

/// Résumé d'une session de conversation passée, tel que proposé dans la
/// liste affichée par [`AgentPanel`] (voir `on_list_sessions`/`sessions`).
/// `label` et `preview` sont déjà mis en forme par la page hôte (date
/// lisible, aperçu tronqué du premier message) : ce composant reste
/// agnostique du format d'horodatage transmis par le serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionSummary {
    pub id: String,
    pub label: String,
    pub preview: Option<String>,
}

/// Transcript reconstruit (lecture seule) d'une session passée, affiché en
/// recouvrement de l'historique courant sans jamais le modifier (voir
/// `on_open_session`/`session_history`) : fermer la consultation restaure
/// l'affichage de `messages` tel quel, intact.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentSessionHistory {
    pub session_id: String,
    pub entries: Vec<PanelEntry>,
}

/// Rôle d'un message du contexte brut envoyé au modèle par le frame
/// Superviseur (voir [`SupervisorContextEntry`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupervisorContextRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Appel d'outil porté par un message assistant du contexte brut (voir
/// [`SupervisorContextEntry::Assistant`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SupervisorContextToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Message brut de l'historique effectivement envoyé au modèle par le frame
/// Superviseur (voir `agent::orchestration::AgentFrame::history`), système
/// compris — contrairement à [`AgentSessionHistory`], qui n'en donne qu'une
/// lecture simplifiée destinée à l'inspecteur, ce type reflète tel quel ce
/// que le Superviseur envoie effectivement au modèle, pour outiller le
/// diagnostic d'un comportement inattendu de l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SupervisorContextEntry {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<SupervisorContextToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// Élément de l'historique affiché par [`AgentPanel`] : message texte,
/// réflexion du modèle, ou trace d'appel d'outil. Contrairement à
/// [`PanelMessage`], qui ne portait que des échanges texte, [`PanelEntry`]
/// permet de tracer en direct ce que fait l'agent entre deux réponses.
///
/// `Hash` (en plus de `PartialEq`/`Eq`) sert de clé de rendu à [`AgentPanel`]
/// : chaque entrée est mutée en place (delta de réflexion, statut d'outil...)
/// plutôt que remplacée, donc `<For>` doit être reclé sur le contenu entier
/// et pas seulement sur la position, sans quoi Leptos ne réconcilie jamais
/// la vue déjà montée avec la mutation (voir `render_panel_entry`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PanelEntry {
    Message(PanelMessage),
    Reasoning(PanelReasoning),
    ToolCall(PanelToolCall),
    Error(PanelError),
}

impl PanelEntry {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::Message(PanelMessage::user(content))
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Message(PanelMessage::assistant(content))
    }

    /// Comme [`Self::assistant`], en précisant le frame (Superviseur ou
    /// expert délégué) à l'origine du message.
    #[must_use]
    pub fn assistant_from(agent_label: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Message(PanelMessage::assistant_from(agent_label, content))
    }

    /// Échec de la tâche agent (voir [`PanelErrorKind::Failure`]).
    #[must_use]
    pub fn error(agent_label: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error(PanelError {
            agent_label: agent_label.into(),
            message: message.into(),
            kind: PanelErrorKind::Failure,
        })
    }

    /// Arrêt volontaire de la tâche agent (voir [`PanelErrorKind::Stopped`]).
    #[must_use]
    pub fn stopped(message: impl Into<String>) -> Self {
        Self::Error(PanelError {
            agent_label: String::new(),
            message: message.into(),
            kind: PanelErrorKind::Stopped,
        })
    }
}

impl From<PanelMessage> for PanelEntry {
    fn from(message: PanelMessage) -> Self {
        Self::Message(message)
    }
}

/// Une question affichée dans le formulaire structuré du panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelQuestion {
    pub id: String,
    pub label: String,
    /// Si `Some`, affiche un sélecteur parmi ces options ; sinon, champ texte libre.
    pub options: Option<Vec<String>>,
}

/// Réponse de l'utilisateur à une question du formulaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelQuestionAnswer {
    pub question_id: String,
    pub value: String,
    /// Si `Some`, l'utilisateur indique que sa réponse n'est pas satisfaisante
    /// et en précise la raison.
    pub unsatisfactory_reason: Option<String>,
}

/// Requête structurée de l'agent demandant à l'utilisateur de remplir un
/// formulaire. Lorsque ce signal est `Some`, le panneau remplace la zone de
/// saisie texte par le formulaire correspondant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionRequest {
    /// Libellé du frame à l'origine de la question (voir [`PanelReasoning::agent_label`]).
    pub agent_label: String,
    pub prompt: String,
    pub questions: Vec<PanelQuestion>,
}

/// Réponse de l'utilisateur à un [`InteractionRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionResponse {
    pub answers: Vec<PanelQuestionAnswer>,
}

/// Requête de l'agent demandant à l'utilisateur de fournir un document
/// externe, upload (outil `request_document`). Distincte de
/// [`InteractionRequest`] : elle affiche un sélecteur de fichier plutôt qu'un
/// formulaire de questions, tant que ce signal est `Some`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRequest {
    /// Libellé du frame à l'origine de la demande (voir [`PanelReasoning::agent_label`]).
    pub agent_label: String,
    pub prompt: String,
    /// Types MIME acceptés par le sélecteur de fichier (attribut `accept`) ;
    /// vide si tous les types sont acceptés.
    pub accepted_mime_types: Vec<String>,
}

/// Document choisi par l'utilisateur en réponse à un [`DocumentRequest`],
/// son contenu encodé en base64 (voir `server::protocol::DocumentUploadWire`,
/// son pendant sur le fil).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentUpload {
    pub file_name: String,
    pub mime_type: String,
    pub content_base64: String,
}

/// Lit le premier fichier sélectionné dans `input` et invoque
/// `on_document_response` une fois son contenu disponible, encodé en base64
/// (voir [`DocumentUpload`]) : `FileReader::read_as_data_url` produit
/// directement une chaîne `data:<mime>;base64,<contenu>`, dont seule la
/// partie après la virgule nous intéresse. Exportée (voir
/// `agent::read_uploaded_file`) : réutilisée telle quelle par
/// `app::pages::project_documents::ProjectFilesPanel` pour l'upload manuel de
/// fichiers de projet, qui n'a pas besoin de sa propre copie de cette lecture
/// de fichier ni des dépendances `web-sys` associées.
pub fn read_uploaded_file(
    input: web_sys::HtmlInputElement,
    on_document_response: Callback<DocumentUpload>,
) {
    use wasm_bindgen::closure::Closure;

    let Some(files) = input.files() else { return };
    let Some(file) = files.get(0) else { return };
    let file_name = file.name();
    let mime_type = file.type_();

    let Ok(reader) = web_sys::FileReader::new() else {
        return;
    };
    let reader_for_closure = reader.clone();
    let onload = Closure::<dyn FnMut()>::new(move || {
        let Ok(result) = reader_for_closure.result() else {
            return;
        };
        let Some(data_url) = result.as_string() else {
            return;
        };
        let content_base64 = data_url
            .split_once(',')
            .map(|(_, encoded)| encoded.to_string())
            .unwrap_or_default();
        on_document_response.run(DocumentUpload {
            file_name: file_name.clone(),
            mime_type: mime_type.clone(),
            content_base64,
        });
    });
    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    let _ = reader.read_as_data_url(&file);
}

#[derive(Clone, Default)]
struct QuestionDraft {
    value: String,
    unsatisfactory: bool,
    reason: String,
}

/// Affiche une [`PanelEntry`] de l'historique : messages texte comme avant,
/// plus les deux nouveaux types de trace (réflexion, appel d'outil), chacun
/// avec sa propre présentation.
fn render_panel_entry(entry: PanelEntry) -> AnyView {
    match entry {
        PanelEntry::Message(message) => match message.role {
            PanelRole::User => view! {
                <p class="self-end bg-blue-france text-white \
                          max-w-[80%] rounded-sm px-3 py-1.5 text-sm">
                    {message.content.clone()}
                </p>
            }
            .into_any(),
            PanelRole::Assistant => {
                let agent_label = message.agent_label.clone();
                view! {
                    <div class="self-start max-w-[80%] flex flex-col gap-1">
                        {(!agent_label.is_empty()).then(|| view! {
                            <Badge severity=Severity::Info small=true>{agent_label}</Badge>
                        })}
                        <div
                            class="markdown-content bg-blue-france-975 dark:bg-gray-800 \
                                   text-gray-900 dark:text-gray-100 \
                                   rounded-sm px-3 py-1.5 text-sm"
                            inner_html=render_markdown(&message.content)
                        ></div>
                    </div>
                }
                .into_any()
            }
        },
        PanelEntry::Reasoning(reasoning) => {
            let done = reasoning.done;
            let agent_label = reasoning.agent_label.clone();
            view! {
                <details class="self-start max-w-[80%] rounded-sm border border-dashed \
                                border-gray-300 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 \
                                text-sm text-gray-500 dark:text-gray-400">
                    <summary class="flex cursor-pointer select-none items-center gap-2 \
                                    px-3 py-1.5 italic">
                        {(!agent_label.is_empty()).then(|| view! {
                            <Badge severity=Severity::Info small=true>{agent_label}</Badge>
                        })}
                        <span>"Réflexion"</span>
                        {(!done).then(|| view! { <span class="animate-pulse">"▍"</span> })}
                    </summary>
                    <div
                        class="markdown-content not-italic px-3 pb-1.5"
                        inner_html=render_markdown(&reasoning.content)
                    ></div>
                </details>
            }
            .into_any()
        }
        PanelEntry::ToolCall(call) => {
            let display_name = tool_display_name(&call.name);
            let summary = tool_call_summary(&call.name, &call.arguments);
            let has_arguments = call.arguments.trim() != "{}";
            let status_badge = match &call.status {
                PanelToolCallStatus::Running => view! {
                    <span class="inline-flex items-center gap-1.5 text-xs italic \
                                 text-gray-500 dark:text-gray-400">
                        <span class="size-1.5 rounded-full bg-gray-400 dark:bg-gray-500 animate-pulse"></span>
                        "en cours…"
                    </span>
                }
                .into_any(),
                PanelToolCallStatus::Done { .. } => view! {
                    <Badge severity=Severity::Success small=true>"Terminé"</Badge>
                }
                .into_any(),
                PanelToolCallStatus::Error { .. } => view! {
                    <Badge severity=Severity::Error small=true>"Échec"</Badge>
                }
                .into_any(),
            };
            let output = match &call.status {
                PanelToolCallStatus::Running => None,
                PanelToolCallStatus::Done { output } => Some(output.clone()),
                PanelToolCallStatus::Error { message } => Some(message.clone()),
            };
            let output_label = match &call.status {
                PanelToolCallStatus::Error { .. } => "Erreur : ",
                _ => "Résultat : ",
            };
            view! {
                <details class="self-start w-full max-w-[90%] rounded-sm border \
                                border-blue-france-925 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 text-xs">
                    <summary class="flex cursor-pointer select-none items-center gap-2 px-2 py-1.5">
                        {(!call.agent_label.is_empty()).then(|| view! {
                            <Badge severity=Severity::Info small=true>{call.agent_label.clone()}</Badge>
                        })}
                        <span class="font-semibold text-gray-700 dark:text-gray-200">{display_name}</span>
                        {summary.map(|s| view! {
                            <span class="truncate italic text-gray-500 dark:text-gray-400">
                                {format!("« {s} »")}
                            </span>
                        })}
                        <span class="ml-auto flex-shrink-0">{status_badge}</span>
                    </summary>
                    <div class="space-y-1 whitespace-pre-wrap break-words border-t \
                                border-blue-france-925 dark:border-gray-700 px-2 py-2 font-mono \
                                text-gray-900 dark:text-gray-100">
                        <div class="text-gray-500 dark:text-gray-400">
                            "Outil : " <span class="font-semibold">{call.name.clone()}</span>
                        </div>
                        {has_arguments.then(|| view! {
                            <div>
                                <span class="text-gray-500 dark:text-gray-400">"Arguments : "</span>
                                {call.arguments.clone()}
                            </div>
                        })}
                        {output.map(|output| view! {
                            <div>
                                <span class="text-gray-500 dark:text-gray-400">{output_label}</span>
                                {output}
                            </div>
                        })}
                    </div>
                </details>
            }
            .into_any()
        }
        PanelEntry::Error(error) => {
            let agent_label = error.agent_label.clone();
            let (severity, title) = match error.kind {
                PanelErrorKind::Failure => (Severity::Error, "Erreur"),
                PanelErrorKind::Stopped => (Severity::Warning, "Interrompu"),
            };
            view! {
                <div class="self-start max-w-[80%] flex flex-col gap-1">
                    {(!agent_label.is_empty()).then(|| view! {
                        <Badge severity=Severity::Info small=true>{agent_label}</Badge>
                    })}
                    <Alert severity=severity title=title small=true>
                        <p class="whitespace-pre-wrap break-words">{error.message.clone()}</p>
                    </Alert>
                </div>
            }
            .into_any()
        }
    }
}

/// Libellé humain d'un [`SupervisorContextRole`], affiché en badge devant
/// chaque message du contexte brut (voir [`render_supervisor_context_entry`]).
fn supervisor_context_role_label(role: SupervisorContextRole) -> &'static str {
    match role {
        SupervisorContextRole::System => "Système",
        SupervisorContextRole::User => "Utilisateur",
        SupervisorContextRole::Assistant => "Assistant",
        SupervisorContextRole::Tool => "Résultat d'outil",
    }
}

/// Affiche un message brut du contexte du Superviseur (voir
/// [`SupervisorContextEntry`]) : rôle en badge, contenu textuel le cas
/// échéant, et appels d'outils demandés pour un message assistant.
fn render_supervisor_context_message(
    role: SupervisorContextRole,
    content: Option<String>,
    tool_calls: Vec<SupervisorContextToolCall>,
) -> AnyView {
    let severity = match role {
        SupervisorContextRole::System => Severity::Warning,
        SupervisorContextRole::User => Severity::Success,
        SupervisorContextRole::Assistant | SupervisorContextRole::Tool => Severity::Info,
    };
    view! {
        <div class="w-full rounded-sm border border-blue-france-925 dark:border-gray-700 \
                    bg-gray-50 dark:bg-gray-800 text-xs px-2 py-1.5">
            <div class="mb-1">
                <Badge severity=severity small=true>{supervisor_context_role_label(role)}</Badge>
            </div>
            {content.map(|content| view! {
                <div class="whitespace-pre-wrap break-words font-mono \
                            text-gray-900 dark:text-gray-100">
                    {content}
                </div>
            })}
            {(!tool_calls.is_empty()).then(|| view! {
                <div class="mt-1 space-y-1">
                    {tool_calls.into_iter().map(|call| view! {
                        <div class="whitespace-pre-wrap break-words font-mono \
                                    text-gray-500 dark:text-gray-400">
                            {format!("→ {}({})", call.name, call.arguments)}
                        </div>
                    }).collect_view()}
                </div>
            })}
        </div>
    }
    .into_any()
}

/// Affiche une [`SupervisorContextEntry`] du contexte brut consulté via
/// [`AgentPanel`]'s `supervisor_context`.
fn render_supervisor_context_entry(entry: SupervisorContextEntry) -> AnyView {
    match entry {
        SupervisorContextEntry::System { content } => render_supervisor_context_message(
            SupervisorContextRole::System,
            Some(content),
            Vec::new(),
        ),
        SupervisorContextEntry::User { content } => render_supervisor_context_message(
            SupervisorContextRole::User,
            Some(content),
            Vec::new(),
        ),
        SupervisorContextEntry::Assistant {
            content,
            tool_calls,
        } => {
            render_supervisor_context_message(SupervisorContextRole::Assistant, content, tool_calls)
        }
        SupervisorContextEntry::ToolResult {
            tool_call_id,
            content,
        } => view! {
            <div class="w-full rounded-sm border border-blue-france-925 dark:border-gray-700 \
                        bg-gray-50 dark:bg-gray-800 text-xs px-2 py-1.5">
                <div class="flex items-center gap-2 mb-1">
                    <Badge severity=Severity::Info small=true>"Résultat d'outil"</Badge>
                    <span class="text-gray-500 dark:text-gray-400 font-mono">{tool_call_id}</span>
                </div>
                <div class="whitespace-pre-wrap break-words font-mono \
                            text-gray-900 dark:text-gray-100">
                    {content}
                </div>
            </div>
        }
        .into_any(),
    }
}

/// Phrases affichées tour à tour dans [`PendingAgent`] pendant que la boucle
/// agentique tourne côté serveur : un ton professionnel mais un peu taquin,
/// pour rendre l'attente moins morne qu'un simple « L'agent réfléchit… »
/// statique.
const AGENT_PENDING_PHRASES: &[&str] = &[
    "L'agent réfléchit…",
    "Consultation discrète des visas en cours…",
    "L'agent pèse chaque mot, comme un bon juriste…",
    "Calibrage de la formule la plus élégante…",
    "Vérification qu'aucune virgule ne sera contestée…",
    "L'agent aligne les articles au carré…",
    "Petit détour par le Journal officiel virtuel…",
    "Recherche du mot juste — celui qui ne fâche personne…",
    "Mise en cohérence de l'acte, considérant par considérant…",
    "L'agent relit une deuxième fois, on ne sait jamais…",
    "Recalcul de la numérotation avec la plus grande rigueur…",
    "Encore un instant, la précision administrative a un prix…",
];

/// Tire un indice pseudo-aléatoire dans `[0, len)`, différent de `previous`
/// si possible, via `Math.random()` : suffisant pour faire varier une phrase
/// d'attente à l'écran, sans tirer une dépendance comme `rand` juste pour ça.
fn random_phrase_index(len: usize, previous: Option<usize>) -> usize {
    if len <= 1 {
        return 0;
    }
    loop {
        let index = (js_sys::Math::random() * len as f64) as usize;
        let index = index.min(len - 1);
        if Some(index) != previous {
            return index;
        }
    }
}

/// Anneau tricolore (bleu France / blanc / rouge Marianne) qui tourne en
/// continu : indicateur d'attente à côté des phrases de [`PendingAgent`].
#[component]
fn TricolorSpinner() -> impl IntoView {
    view! {
        <span class="relative inline-block size-4 shrink-0" role="status" aria-hidden="true">
            <span
                class="absolute inset-0 rounded-full animate-spin shadow-[0_0_0_1px_rgba(0,0,0,0.15)]"
                style="background: conic-gradient(var(--color-blue-france) 0deg 120deg, \
                       #ffffff 120deg 240deg, var(--color-red-marianne) 240deg 360deg);"
            ></span>
            <span class="absolute inset-[3px] rounded-full bg-blue-france-975 dark:bg-gray-800"></span>
        </span>
    }
}

#[component]
fn PendingAgent() -> impl IntoView {
    let phrase_index = RwSignal::new(random_phrase_index(AGENT_PENDING_PHRASES.len(), None));

    if let Ok(handle) = leptos::prelude::set_interval_with_handle(
        move || {
            phrase_index.update(|index| {
                *index = random_phrase_index(AGENT_PENDING_PHRASES.len(), Some(*index));
            });
        },
        std::time::Duration::from_millis(2600),
    ) {
        on_cleanup(move || handle.clear());
    }

    view! {
        <p class="self-start max-w-[80%] flex items-center gap-2 rounded-sm px-3 py-1.5 text-sm italic \
                bg-blue-france-975 dark:bg-gray-800 text-gray-600 dark:text-gray-400">
            <TricolorSpinner/>
            {move || AGENT_PENDING_PHRASES[phrase_index.get()]}
        </p>
    }
}

/// Panneau de discussion avec l'agent IA : historique des échanges, zone de
/// saisie et indicateur d'attente pendant l'exécution de la boucle
/// agentique côté serveur.
///
/// Lorsque `interaction` est `Some`, la zone de saisie texte est remplacée
/// par un formulaire structuré ; la soumission du formulaire déclenche
/// `on_respond`. Les messages texte libres (hors interaction) déclenchent
/// `on_send`.
#[component]
pub fn AgentPanel(
    /// Historique des échanges, tenu par la page hôte : messages texte,
    /// réflexions du modèle et traces d'appels d'outils entremêlés dans leur
    /// ordre d'arrivée (voir [`PanelEntry`]).
    #[prop(into)]
    messages: Signal<Vec<PanelEntry>>,
    /// `true` tant que l'agent n'a pas renvoyé sa réponse finale.
    #[prop(into)]
    pending: Signal<bool>,
    /// Invoqué avec le texte saisi lorsque l'utilisateur envoie un message libre.
    on_send: impl Fn(String) + Clone + Send + 'static,
    /// Formulaire structuré à afficher lorsque l'agent attend des réponses
    /// précises de l'utilisateur.
    #[prop(optional, into)]
    interaction: Option<Signal<Option<InteractionRequest>>>,
    /// Invoqué avec les réponses du formulaire lorsque l'utilisateur valide.
    #[prop(optional)]
    on_respond: Option<Callback<InteractionResponse>>,
    /// `true` si l'utilisateur a choisi d'accepter automatiquement toutes
    /// les modifications proposées par l'agent, sans confirmation
    /// individuelle. Si absent, la case à cocher correspondante n'est pas
    /// affichée.
    #[prop(optional, into)]
    auto_accept: Option<Signal<bool>>,
    /// Invoqué avec la nouvelle valeur lorsque l'utilisateur bascule la case
    /// « accepter toutes les modifications ».
    #[prop(optional)]
    on_toggle_auto_accept: Option<Callback<bool>>,
    /// Invoqué lorsque l'utilisateur choisit d'effacer l'historique de la
    /// conversation (voir `app::ws::RoomHandle::clear_history`). Si absent,
    /// le bouton correspondant n'est pas affiché.
    #[prop(optional)]
    on_clear_history: Option<Callback<()>>,
    /// Invoqué lorsque l'utilisateur demande l'arrêt immédiat de la tâche
    /// agent en cours (voir `app::ws::RoomHandle::stop_agent`). Si absent,
    /// aucun bouton « Arrêter » n'est affiché ; sans effet si `pending` est
    /// `false`.
    #[prop(optional)]
    on_stop: Option<Callback<()>>,
    /// Invoqué lorsque l'utilisateur demande de relancer la dernière tâche
    /// envoyée à l'agent, par exemple après un arrêt ou une erreur (voir
    /// `app::ws::RoomHandle::restart_agent`). Si absent, aucun bouton
    /// « Redémarrer » n'est affiché.
    #[prop(optional)]
    on_restart: Option<Callback<()>>,
    /// Requête de document à afficher lorsque l'agent attend un upload
    /// (outil `request_document`). Comme `interaction`, remplace la zone de
    /// saisie texte tant qu'elle est `Some`.
    #[prop(optional, into)]
    document_request: Option<Signal<Option<DocumentRequest>>>,
    /// Invoqué avec le fichier choisi par l'utilisateur lorsqu'un
    /// `document_request` est en cours.
    #[prop(optional)]
    on_document_response: Option<Callback<DocumentUpload>>,
    /// Sessions de conversation passées de l'utilisateur courant pour ce
    /// projet (voir [`AgentSessionSummary`]), tenues par la page hôte. Si
    /// absent, aucun bouton « Sessions » n'est affiché.
    #[prop(optional, into)]
    sessions: Option<Signal<Vec<AgentSessionSummary>>>,
    /// Invoqué lorsque l'utilisateur ouvre la liste des sessions passées, pour
    /// que la page hôte la (re)charge (voir `app::ws::RoomHandle::list_agent_sessions`).
    #[prop(optional)]
    on_list_sessions: Option<Callback<()>>,
    /// Invoqué avec l'identifiant de la session choisie par l'utilisateur
    /// dans la liste.
    #[prop(optional)]
    on_open_session: Option<Callback<String>>,
    /// Transcript d'une session passée à afficher en lecture seule,
    /// recouvrant temporairement l'historique courant sans jamais le modifier
    /// (voir [`AgentSessionHistory`]) : la conversation en cours (`messages`)
    /// reste intacte, prête à réapparaître à la fermeture.
    #[prop(optional, into)]
    session_history: Option<Signal<Option<AgentSessionHistory>>>,
    /// Invoqué lorsque l'utilisateur ferme la consultation d'une session
    /// passée.
    #[prop(optional)]
    on_close_session_history: Option<Callback<()>>,
    /// Contexte brut (historique `agent::ChatMessage`, système compris) que
    /// le frame Superviseur envoie effectivement au modèle, tel que renvoyé
    /// par la page hôte après [`Self`]'s `on_view_supervisor_context` :
    /// `None` tant qu'il n'a pas encore été demandé ou chargé. Affiché en
    /// recouvrement de l'historique courant, comme `session_history`, sans
    /// jamais le modifier.
    #[prop(optional, into)]
    supervisor_context: Option<Signal<Option<Vec<SupervisorContextEntry>>>>,
    /// Invoqué lorsque l'utilisateur demande à visualiser le contexte du
    /// Superviseur, pour que la page hôte le (re)charge. Si absent, aucun
    /// bouton « Contexte » n'est affiché.
    #[prop(optional)]
    on_view_supervisor_context: Option<Callback<()>>,
    /// Invoqué lorsque l'utilisateur ferme la consultation du contexte du
    /// Superviseur.
    #[prop(optional)]
    on_close_supervisor_context: Option<Callback<()>>,
) -> impl IntoView {
    let (draft, set_draft) = signal(String::new());
    let draft_answers = RwSignal::new(Vec::<QuestionDraft>::new());
    let interaction_panel_width = RwSignal::new(INTERACTION_PANEL_DEFAULT_WIDTH);
    let document_panel_width = RwSignal::new(INTERACTION_PANEL_DEFAULT_WIDTH);
    let sessions_open = RwSignal::new(false);

    let interaction = interaction.unwrap_or_else(|| Signal::derive(|| None));
    let document_request = document_request.unwrap_or_else(|| Signal::derive(|| None));
    let sessions = sessions.unwrap_or_else(|| Signal::derive(Vec::new));
    let session_history = session_history.unwrap_or_else(|| Signal::derive(|| None));
    let supervisor_context = supervisor_context.unwrap_or_else(|| Signal::derive(|| None));

    // Réinitialise les brouillons quand un nouveau formulaire apparaît.
    Effect::new(move |_| {
        let count = interaction
            .get()
            .as_ref()
            .map(|req| req.questions.len())
            .unwrap_or(0);
        draft_answers.set(vec![QuestionDraft::default(); count]);
    });

    let make_send = move || {
        let on_send = on_send.clone();
        move || {
            let text = draft.get().trim().to_string();
            if text.is_empty() || pending.get() {
                return;
            }
            on_send(text);
            set_draft.set(String::new());
        }
    };

    let has_sessions = on_list_sessions.is_some() && on_open_session.is_some();

    view! {
        <div class="relative flex flex-col h-full border border-blue-france-925 dark:border-gray-700 \
                    bg-white dark:bg-gray-900 rounded-sm overflow-hidden">

            // En-tête
            <div class="relative px-3 py-2 border-b border-blue-france-925 dark:border-gray-700 \
                        bg-blue-france-975 dark:bg-gray-800 flex-shrink-0">
                <p class="text-sm font-bold text-blue-france dark:text-blue-france-925 flex items-baseline gap-2">
                    <span class="flex-1 uppercase">Marie</span>
                    <Badge severity=Severity::Info>IA</Badge>
                    {has_sessions.then(|| view! {
                        <button
                            type="button"
                            title="Consulter une conversation passée"
                            class="text-xs font-normal normal-case text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer"
                            on:click=move |_| {
                                let now_open = !sessions_open.get_untracked();
                                sessions_open.set(now_open);
                                if now_open && let Some(on_list) = on_list_sessions {
                                    on_list.run(());
                                }
                            }
                        >
                            "Sessions"
                        </button>
                    })}
                    {on_view_supervisor_context.map(|on_view| view! {
                        <button
                            type="button"
                            title="Visualiser le contexte brut envoyé au modèle par le Superviseur"
                            class="text-xs font-normal normal-case text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer"
                            on:click=move |_| on_view.run(())
                        >
                            "Contexte"
                        </button>
                    })}
                    {on_clear_history.map(|on_clear| view! {
                        <button
                            type="button"
                            title="Archiver la conversation en cours et en commencer une nouvelle"
                            class="text-xs font-normal normal-case text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer disabled:cursor-not-allowed \
                                   disabled:opacity-40 disabled:hover:no-underline"
                            disabled=move || pending.get() || messages.get().is_empty()
                            on:click=move |_| on_clear.run(())
                        >
                            "Nouvelle session"
                        </button>
                    })}
                    {on_stop.map(|on_stop| view! {
                        <button
                            type="button"
                            title="Arrêter immédiatement la tâche en cours"
                            class="text-xs font-normal normal-case text-error \
                                   hover:underline underline-offset-2 cursor-pointer disabled:cursor-not-allowed \
                                   disabled:opacity-40 disabled:hover:no-underline"
                            disabled=move || !pending.get()
                            on:click=move |_| on_stop.run(())
                        >
                            "Arrêter"
                        </button>
                    })}
                    {on_restart.map(|on_restart| view! {
                        <button
                            type="button"
                            title="Relancer la dernière tâche envoyée à l'agent"
                            class="text-xs font-normal normal-case text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer disabled:cursor-not-allowed \
                                   disabled:opacity-40 disabled:hover:no-underline"
                            disabled=move || pending.get() || messages.get().is_empty()
                            on:click=move |_| on_restart.run(())
                        >
                            "Redémarrer"
                        </button>
                    })}
                </p>
                {move || {
                    match (auto_accept, on_toggle_auto_accept) {
                        (Some(auto_accept), Some(on_toggle)) => Some(view! {
                            <label class="flex items-center gap-2 mt-1 text-xs text-gray-700 dark:text-gray-200 cursor-pointer">
                                <input
                                    type="checkbox"
                                    class="size-4 accent-blue-france cursor-pointer"
                                    prop:checked=move || auto_accept.get()
                                    on:change=move |ev| on_toggle.run(event_target_checked(&ev))
                                />
                                "Accepter automatiquement toutes les modifications"
                            </label>
                        }.into_any()),
                        _ => None,
                    }
                }}

                // Liste déroulante des sessions passées : positionnée sous le
                // bouton « Sessions », sans effet sur la conversation
                // affichée tant qu'aucun élément n'est choisi (voir
                // `on_open_session`).
                {move || sessions_open.get().then(|| {
                    let items = sessions.get();
                    view! {
                        <div class="absolute right-3 top-full z-20 mt-1 max-h-72 w-72 overflow-y-auto \
                                    rounded-sm border border-blue-france-925 dark:border-gray-700 \
                                    bg-white dark:bg-gray-900 shadow-lg text-sm">
                            {if items.is_empty() {
                                view! {
                                    <p class="px-3 py-2 italic text-gray-500 dark:text-gray-400">
                                        "Aucune session archivée pour l'instant."
                                    </p>
                                }.into_any()
                            } else {
                                items.into_iter().map(|session| {
                                    let label = session.label.clone();
                                    let preview = session.preview.clone();
                                    let session_id = session.id.clone();
                                    view! {
                                        <button
                                            type="button"
                                            class="block w-full text-left px-3 py-2 border-b last:border-b-0 \
                                                   border-blue-france-925 dark:border-gray-700 \
                                                   hover:bg-blue-france-975 dark:hover:bg-gray-800 cursor-pointer"
                                            on:click=move |_| {
                                                sessions_open.set(false);
                                                if let Some(on_open) = on_open_session {
                                                    on_open.run(session_id.clone());
                                                }
                                            }
                                        >
                                            <span class="block font-semibold text-gray-900 dark:text-gray-100">{label}</span>
                                            {preview.map(|p| view! {
                                                <span class="block truncate italic text-gray-500 dark:text-gray-400">{p}</span>
                                            })}
                                        </button>
                                    }
                                }).collect_view().into_any()
                            }}
                        </div>
                    }
                })}
            </div>

            // Recouvrement en lecture seule du transcript d'une session
            // passée : `messages`/`pending` restent inchangés en dessous, ils
            // réapparaissent tels quels à la fermeture (voir
            // `on_close_session_history`).
            {move || session_history.get().map(|history| view! {
                <div class="absolute inset-0 z-10 flex flex-col bg-white dark:bg-gray-900">
                    <div class="flex items-center gap-2 px-3 py-2 border-b border-blue-france-925 \
                                dark:border-gray-700 bg-blue-france-975 dark:bg-gray-800 flex-shrink-0">
                        <Badge severity=Severity::Info small=true>"Lecture seule"</Badge>
                        <span class="flex-1 text-sm font-bold text-blue-france dark:text-blue-france-925">
                            "Session archivée"
                        </span>
                        <button
                            type="button"
                            class="text-xs font-normal text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer"
                            on:click=move |_| {
                                if let Some(on_close) = on_close_session_history {
                                    on_close.run(());
                                }
                            }
                        >
                            "Fermer"
                        </button>
                    </div>
                    <div class="flex-1 overflow-y-auto p-3 flex flex-col gap-2">
                        <For
                            each=move || history.entries.clone().into_iter().enumerate()
                            key=|(index, entry)| (*index, entry.clone())
                            children=move |(_, entry)| render_panel_entry(entry)
                        />
                    </div>
                </div>
            })}

            // Recouvrement en lecture seule du contexte brut du Superviseur
            // (voir `supervisor_context`) : même principe que le recouvrement
            // de session archivée ci-dessus, sans effet sur `messages`.
            {move || supervisor_context.get().map(|entries| view! {
                <div class="absolute inset-0 z-10 flex flex-col bg-white dark:bg-gray-900">
                    <div class="flex items-center gap-2 px-3 py-2 border-b border-blue-france-925 \
                                dark:border-gray-700 bg-blue-france-975 dark:bg-gray-800 flex-shrink-0">
                        <Badge severity=Severity::Info small=true>"Lecture seule"</Badge>
                        <span class="flex-1 text-sm font-bold text-blue-france dark:text-blue-france-925">
                            "Contexte du Superviseur"
                        </span>
                        <button
                            type="button"
                            class="text-xs font-normal text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer"
                            on:click=move |_| {
                                if let Some(on_close) = on_close_supervisor_context {
                                    on_close.run(());
                                }
                            }
                        >
                            "Fermer"
                        </button>
                    </div>
                    <div class="flex-1 overflow-y-auto p-3 flex flex-col gap-2">
                        <For
                            each=move || entries.clone().into_iter().enumerate()
                            key=|(index, entry)| (*index, entry.clone())
                            children=move |(_, entry)| render_supervisor_context_entry(entry)
                        />
                    </div>
                </div>
            })}

            <div class="flex-1 flex overflow-hidden min-h-0">
                // Colonne chat : historique des messages et saisie libre.
                // Redimensionnable indépendamment du formulaire (voir
                // colonne de droite) via la poignée de glissement partagée
                // avec le panneau agent principal (`ResizeHandle`).
                <div class="flex-1 flex flex-col overflow-hidden min-w-0">
                    <div class="flex-1 overflow-y-auto p-3 flex flex-col gap-2">
                        <For
                            each=move || messages.get().into_iter().enumerate()
                            key=|(index, entry)| (*index, entry.clone())
                            children=move |(_, entry)| render_panel_entry(entry)
                        />
                        {move || pending.get().then(|| view! { <PendingAgent/> })}
                    </div>

                    // Saisie texte libre, masquée quand un formulaire structuré
                    // ou un sélecteur de document est affiché à côté (colonne
                    // de droite ci-dessous).
                    {move || (interaction.get().is_none() && document_request.get().is_none()).then(|| {
                        let send = make_send();
                        let send_on_click = make_send();
                        view! {
                            <div class="flex gap-2 items-center px-3 py-2 border-t border-blue-france-925 dark:border-gray-700 flex-shrink-0">
                                <input
                                    type="text"
                                    class="flex-1 shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                           focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                           bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 \
                                           px-3 py-2 outline-none disabled:opacity-50"
                                    placeholder="Demander à l'agent…"
                                    prop:value=draft
                                    prop:disabled=move || pending.get()
                                    on:input=move |ev| set_draft.set(event_target_value(&ev))
                                    on:keydown=move |ev: KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            send();
                                        }
                                    }
                                />
                                <button
                                    type="button"
                                    class="bg-blue-france text-white hover:bg-blue-france-hover \
                                           active:bg-blue-france-active text-sm px-3 py-2 \
                                           inline-flex items-center font-bold rounded-sm cursor-pointer \
                                           transition-colors disabled:cursor-not-allowed disabled:opacity-40"
                                    disabled=move || pending.get() || draft.get().trim().is_empty()
                                    on:click=move |_| send_on_click()
                                >
                                    "Envoyer"
                                </button>
                            </div>
                        }
                    })}
                </div>

                // Colonne formulaire : n'apparaît que lorsque l'agent attend des
                // réponses précises de l'utilisateur ; redimensionnable par
                // glissement indépendamment de la colonne chat.
                {move || {
                if let Some(req) = interaction.get() {
                    let agent_label = req.agent_label.clone();
                    let prompt = req.prompt.clone();
                    let fields = req.questions.into_iter().enumerate().map(|(i, q)| {
                        let q_label   = q.label.clone();
                        let q_opts    = q.options.clone();

                        let value         = move || draft_answers.with(|ds| ds.get(i).map(|d| d.value.clone()).unwrap_or_default());
                        let set_value     = move |v: String| draft_answers.update(|ds| { if let Some(d) = ds.get_mut(i) { d.value = v; } });
                        let set_value_ev  = set_value;

                        let unsat        = move || draft_answers.with(|ds| ds.get(i).map(|d| d.unsatisfactory).unwrap_or(false));
                        let toggle_unsat = move |_| draft_answers.update(|ds| {
                            if let Some(d) = ds.get_mut(i) {
                                d.unsatisfactory = !d.unsatisfactory;
                                if !d.unsatisfactory { d.reason.clear(); }
                            }
                        });

                        let reason     = move || draft_answers.with(|ds| ds.get(i).map(|d| d.reason.clone()).unwrap_or_default());
                        let set_reason = move |r: String| draft_answers.update(|ds| { if let Some(d) = ds.get_mut(i) { d.reason = r; } });

                        let field_id  = format!("aq-{}", q.id);
                        let unsat_id  = format!("aq-unsat-{}", q.id);
                        let reason_id = format!("aq-reason-{}", q.id);

                        view! {
                            <div class="flex flex-col gap-1">
                                <label class="text-sm font-bold text-gray-900 dark:text-gray-100" for=field_id.clone()>
                                    {q_label}
                                </label>
                                {if let Some(opts) = q_opts {
                                    view! {
                                        <select
                                            id=field_id
                                            class="shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                                   focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                                   bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 \
                                                   px-3 py-2 outline-none disabled:opacity-50 w-full"
                                            prop:value=value
                                            prop:disabled=move || pending.get()
                                            on:change=move |ev| set_value(event_target_value(&ev))
                                        >
                                            <option value="" disabled=true selected=move || value().is_empty()>
                                                "Sélectionner…"
                                            </option>
                                            {opts.into_iter().map(|opt| {
                                                let opt_clone = opt.clone();
                                                view! {
                                                    <option
                                                        value=opt_clone.clone()
                                                        selected=move || value() == opt_clone
                                                    >
                                                        {opt}
                                                    </option>
                                                }
                                            }).collect_view()}
                                        </select>
                                    }.into_any()
                                } else {
                                    view! {
                                        <input
                                            id=field_id
                                            type="text"
                                            class="shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                                   focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                                   bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 \
                                                   px-3 py-2 outline-none disabled:opacity-50 w-full"
                                            prop:value=value
                                            prop:disabled=move || pending.get()
                                            on:input=move |ev| set_value_ev(event_target_value(&ev))
                                        />
                                    }.into_any()
                                }}
                                <label class="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-200 cursor-pointer"
                                    for=unsat_id.clone()>
                                    <input
                                        id=unsat_id.clone()
                                        type="checkbox"
                                        class="size-5 accent-blue-france cursor-pointer"
                                        prop:checked=unsat
                                        prop:disabled=move || pending.get()
                                        on:change=toggle_unsat
                                    />
                                    "Réponse non satisfaisante"
                                </label>
                                {move || unsat().then(|| view! {
                                    <div class="flex flex-col gap-1">
                                        <label class="text-sm font-bold text-gray-900 dark:text-gray-100" for=reason_id.clone()>
                                            "Précisez pourquoi"
                                        </label>
                                        <textarea
                                            id=reason_id.clone()
                                            class="shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                                   focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                                   bg-gray-100 dark:bg-gray-800 text-gray-900 dark:text-gray-100 \
                                                   px-3 py-2 outline-none disabled:opacity-50 \
                                                   w-full resize-none"
                                            rows="2"
                                            prop:value=reason
                                            prop:disabled=move || pending.get()
                                            on:input=move |ev| set_reason(event_target_value(&ev))
                                        ></textarea>
                                    </div>
                                })}
                            </div>
                        }
                    }).collect_view();

                    let on_respond_cb = on_respond;
                    let submit = move |_| {
                        if let Some(cb) = &on_respond_cb
                            && let Some(req) = interaction.get() {
                                let answers = draft_answers.with(|ds| {
                                    req.questions.iter().zip(ds.iter()).map(|(q, d)| PanelQuestionAnswer {
                                        question_id: q.id.clone(),
                                        value: d.value.clone(),
                                        unsatisfactory_reason: d.unsatisfactory.then(|| d.reason.clone()),
                                    }).collect()
                                });
                                cb.run(InteractionResponse { answers });
                            }
                    };

                    Some(view! {
                        <div class="contents">
                            <ResizeHandle
                                width=interaction_panel_width
                                min_width=INTERACTION_PANEL_MIN_WIDTH
                                max_width=INTERACTION_PANEL_MAX_WIDTH
                            />
                            <div
                                class="shrink-0 overflow-y-auto p-3 flex flex-col gap-3 \
                                       border-l border-blue-france-925 dark:border-gray-700"
                                style:width=move || format!("{}px", interaction_panel_width.get())
                            >
                                {(!agent_label.is_empty()).then(|| view! {
                                    <Badge severity=Severity::Info small=true>{agent_label}</Badge>
                                })}
                                <p class="text-sm italic text-gray-600 dark:text-gray-400">{prompt}</p>
                                {fields}
                                <div class="flex justify-end">
                                    <button
                                        type="button"
                                        class="bg-blue-france text-white hover:bg-blue-france-hover \
                                               active:bg-blue-france-active text-sm px-3 py-1.5 \
                                               inline-flex items-center justify-center font-bold \
                                               rounded-sm cursor-pointer transition-colors \
                                               disabled:cursor-not-allowed disabled:opacity-40"
                                        disabled=move || pending.get()
                                        on:click=submit
                                    >
                                        "Valider"
                                    </button>
                                </div>
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

                // Colonne document : n'apparaît que lorsque l'agent attend un
                // upload (outil `request_document`) ; redimensionnable comme
                // la colonne formulaire ci-dessus.
                {move || {
                    if let Some(req) = document_request.get() {
                        let agent_label = req.agent_label.clone();
                        let prompt = req.prompt.clone();
                        let accept = req.accepted_mime_types.join(",");
                        Some(view! {
                            <div class="contents">
                                <ResizeHandle
                                    width=document_panel_width
                                    min_width=INTERACTION_PANEL_MIN_WIDTH
                                    max_width=INTERACTION_PANEL_MAX_WIDTH
                                />
                                <div
                                    class="shrink-0 overflow-y-auto p-3 flex flex-col gap-3 \
                                           border-l border-blue-france-925 dark:border-gray-700"
                                    style:width=move || format!("{}px", document_panel_width.get())
                                >
                                    {(!agent_label.is_empty()).then(|| view! {
                                        <Badge severity=Severity::Info small=true>{agent_label}</Badge>
                                    })}
                                    <p class="text-sm italic text-gray-600 dark:text-gray-400">{prompt}</p>
                                    <input
                                        type="file"
                                        accept=accept
                                        class="text-sm text-gray-900 dark:text-gray-100"
                                        on:change=move |ev| {
                                            if let Some(cb) = on_document_response
                                                && let Some(input) = ev
                                                    .target()
                                                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                            {
                                                read_uploaded_file(input, cb);
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
            </div>
        </div>
    }
}
