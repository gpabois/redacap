//! Panneau de discussion avec l'agent IA, sous forme de composant Leptos
//! autonome : il ignore tout de la boucle agentique ou du modèle de
//! langage utilisés, et ne fait qu'afficher un historique de messages et
//! relayer la saisie de l'utilisateur via `on_send`. La page hôte reste
//! responsable de l'appel réel à l'agent (typiquement via une fonction
//! serveur Leptos) et de la mise à jour de `messages`/`pending` en retour.

use dsfr::{Badge, ResizeHandle, Severity};
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
        "list_intentions" => "Liste des intentions du projet",
        "add_intention" => "Association d'une intention au projet",
        "remove_intention" => "Retrait d'une intention du projet",
        "ask_user" => "Question posée à l'utilisateur",
        "ask_questions" => "Formulaire présenté à l'utilisateur",
        "request_document" => "Demande d'un document à l'utilisateur",
        "read_document" => "Lecture d'un document externe",
        "legifrance_search" => "Recherche Légifrance",
        "legifrance_fetch" => "Lecture d'un texte Légifrance",
        "georisques_query" => "Interrogation GéoRisques",
        "icpe_query" => "Interrogation de la base ICPE",
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
        "add_intention" | "remove_intention" => &["intention_id"],
        "legifrance_search" => &["query"],
        "legifrance_fetch" => &["textId", "textCid", "id"],
        "request_document" => &["prompt"],
        "ask_user" => &["question"],
        "read_document" => &["document_id", "url"],
        "georisques_query" => &["code_insee", "latlon"],
        "icpe_query" => &["nom_etablissement", "code_insee"],
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
}

impl PanelMessage {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: PanelRole::User,
            content: content.into(),
        }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: PanelRole::Assistant,
            content: content.into(),
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
/// partie après la virgule nous intéresse.
fn read_uploaded_file(input: web_sys::HtmlInputElement, on_document_response: Callback<DocumentUpload>) {
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
            PanelRole::Assistant => view! {
                <div
                    class="markdown-content self-start bg-blue-france-975 dark:bg-gray-800 \
                           text-gray-900 dark:text-gray-100 \
                           max-w-[80%] rounded-sm px-3 py-1.5 text-sm"
                    inner_html=render_markdown(&message.content)
                ></div>
            }
            .into_any(),
        },
        PanelEntry::Reasoning(reasoning) => {
            let done = reasoning.done;
            let agent_label = reasoning.agent_label.clone();
            view! {
                <div class="self-start max-w-[80%] rounded-sm border border-dashed \
                            border-gray-300 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 \
                            px-3 py-1.5 text-sm italic text-gray-500 dark:text-gray-400">
                    {(!agent_label.is_empty()).then(|| view! {
                        <div class="mb-1 not-italic">
                            <Badge severity=Severity::Info small=true>{agent_label}</Badge>
                        </div>
                    })}
                    {reasoning.content.clone()}
                    {(!done).then(|| view! { <span class="animate-pulse">"▍"</span> })}
                </div>
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
    }
}

#[component]
fn PendingAgent() -> impl IntoView {
    view! {
        <p class="self-start max-w-[80%] rounded-sm px-3 py-1.5 text-sm italic \
                bg-blue-france-975 dark:bg-gray-800 text-gray-600 dark:text-gray-400">
            "L'agent réfléchit…"
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
    /// Requête de document à afficher lorsque l'agent attend un upload
    /// (outil `request_document`). Comme `interaction`, remplace la zone de
    /// saisie texte tant qu'elle est `Some`.
    #[prop(optional, into)]
    document_request: Option<Signal<Option<DocumentRequest>>>,
    /// Invoqué avec le fichier choisi par l'utilisateur lorsqu'un
    /// `document_request` est en cours.
    #[prop(optional)]
    on_document_response: Option<Callback<DocumentUpload>>,
) -> impl IntoView {
    let (draft, set_draft) = signal(String::new());
    let draft_answers = RwSignal::new(Vec::<QuestionDraft>::new());
    let interaction_panel_width = RwSignal::new(INTERACTION_PANEL_DEFAULT_WIDTH);
    let document_panel_width = RwSignal::new(INTERACTION_PANEL_DEFAULT_WIDTH);

    let interaction = interaction.unwrap_or_else(|| Signal::derive(|| None));
    let document_request = document_request.unwrap_or_else(|| Signal::derive(|| None));

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

    view! {
        <div class="flex flex-col h-full border border-blue-france-925 dark:border-gray-700 \
                    bg-white dark:bg-gray-900 rounded-sm overflow-hidden">

            // En-tête
            <div class="px-3 py-2 border-b border-blue-france-925 dark:border-gray-700 \
                        bg-blue-france-975 dark:bg-gray-800 flex-shrink-0">
                <p class="text-sm font-bold text-blue-france dark:text-blue-france-925 flex items-baseline gap-2">
                    <span class="flex-1 uppercase">Marie</span>
                    <Badge severity=Severity::Info>IA</Badge>
                    {on_clear_history.map(|on_clear| view! {
                        <button
                            type="button"
                            title="Effacer l'historique de la conversation"
                            class="text-xs font-normal normal-case text-gray-500 dark:text-gray-400 \
                                   hover:text-blue-france dark:hover:text-blue-france-925 \
                                   underline-offset-2 hover:underline cursor-pointer disabled:cursor-not-allowed \
                                   disabled:opacity-40 disabled:hover:no-underline"
                            disabled=move || pending.get() || messages.get().is_empty()
                            on:click=move |_| on_clear.run(())
                        >
                            "Effacer"
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
            </div>

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
