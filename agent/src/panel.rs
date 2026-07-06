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
use web_sys::KeyboardEvent;

/// Largeur initiale du panneau de formulaire d'interaction, en pixels,
/// lorsqu'il est affiché à côté de l'historique de chat.
const INTERACTION_PANEL_DEFAULT_WIDTH: f64 = 280.0;
/// Bornes de largeur autorisées lors du redimensionnement par glissement
/// (voir [`ResizeHandle`]).
const INTERACTION_PANEL_MIN_WIDTH: f64 = 200.0;
const INTERACTION_PANEL_MAX_WIDTH: f64 = 480.0;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelRole {
    User,
    Assistant,
}

/// Message affiché dans l'historique du panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelReasoning {
    pub content: String,
    pub done: bool,
}

/// État d'un appel d'outil tracé dans l'historique (voir [`PanelToolCall`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelToolCallStatus {
    /// L'outil est en cours d'exécution (éventuellement en attente de
    /// confirmation de l'utilisateur, voir [`InteractionRequest`]).
    Running,
    Done { output: String },
    Error { message: String },
}

/// Trace d'un appel d'outil déclenché par l'agent, affichée dans
/// l'historique au même titre qu'un message : nom, arguments et résultat
/// (une fois disponible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelToolCall {
    pub id: String,
    pub name: String,
    /// Arguments de l'appel, tels que sérialisés (JSON) par l'appelant.
    pub arguments: String,
    pub status: PanelToolCallStatus,
}

/// Élément de l'historique affiché par [`AgentPanel`] : message texte,
/// réflexion du modèle, ou trace d'appel d'outil. Contrairement à
/// [`PanelMessage`], qui ne portait que des échanges texte, [`PanelEntry`]
/// permet de tracer en direct ce que fait l'agent entre deux réponses.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub prompt: String,
    pub questions: Vec<PanelQuestion>,
}

/// Réponse de l'utilisateur à un [`InteractionRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionResponse {
    pub answers: Vec<PanelQuestionAnswer>,
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
                    class="markdown-content self-start bg-blue-france-975 text-gray-900 \
                           max-w-[80%] rounded-sm px-3 py-1.5 text-sm"
                    inner_html=render_markdown(&message.content)
                ></div>
            }
            .into_any(),
        },
        PanelEntry::Reasoning(reasoning) => {
            let done = reasoning.done;
            view! {
                <div class="self-start max-w-[80%] rounded-sm border border-dashed \
                            border-gray-300 bg-gray-50 px-3 py-1.5 text-sm italic text-gray-500">
                    {reasoning.content.clone()}
                    {(!done).then(|| view! { <span class="animate-pulse">"▍"</span> })}
                </div>
            }
            .into_any()
        }
        PanelEntry::ToolCall(call) => {
            let (status_label, status_class) = match &call.status {
                PanelToolCallStatus::Running => ("en cours…", "text-gray-500"),
                PanelToolCallStatus::Done { .. } => ("terminé", "text-success"),
                PanelToolCallStatus::Error { .. } => ("erreur", "text-error"),
            };
            let output = match &call.status {
                PanelToolCallStatus::Running => None,
                PanelToolCallStatus::Done { output } => Some(output.clone()),
                PanelToolCallStatus::Error { message } => Some(message.clone()),
            };
            view! {
                <details class="self-start w-full max-w-[90%] rounded-sm border \
                                border-blue-france-925 bg-gray-50 text-xs">
                    <summary class="flex cursor-pointer select-none items-center gap-2 px-2 py-1">
                        <span class="font-mono font-bold text-gray-700">{call.name.clone()}</span>
                        <span class=format!("italic {status_class}")>{status_label}</span>
                    </summary>
                    <div class="space-y-1 whitespace-pre-wrap break-words px-2 pb-2 font-mono">
                        <div>
                            <span class="text-gray-500">"Arguments : "</span>
                            {call.arguments.clone()}
                        </div>
                        {output.map(|output| view! {
                            <div>
                                <span class="text-gray-500">"Résultat : "</span>
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
                bg-blue-france-975 text-gray-600">
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
) -> impl IntoView {
    let (draft, set_draft) = signal(String::new());
    let draft_answers = RwSignal::new(Vec::<QuestionDraft>::new());
    let interaction_panel_width = RwSignal::new(INTERACTION_PANEL_DEFAULT_WIDTH);

    let interaction = interaction.unwrap_or_else(|| Signal::derive(|| None));

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
        <div class="flex flex-col h-full border border-blue-france-925 rounded-sm overflow-hidden">

            // En-tête
            <div class="px-3 py-2 border-b border-blue-france-925 bg-blue-france-975 flex-shrink-0">
                <p class="text-sm font-bold text-blue-france flex items-baseline">
                    <span class="flex-1 uppercase">Marie</span>
                    <Badge severity=Severity::Info>IA</Badge>
                </p>
                {move || {
                    match (auto_accept, on_toggle_auto_accept) {
                        (Some(auto_accept), Some(on_toggle)) => Some(view! {
                            <label class="flex items-center gap-2 mt-1 text-xs text-gray-700 cursor-pointer">
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
                            key=|(index, _)| *index
                            children=move |(_, entry)| render_panel_entry(entry)
                        />
                        {move || pending.get().then(|| view! { <PendingAgent/> })}
                    </div>

                    // Saisie texte libre, masquée quand un formulaire structuré
                    // est affiché à côté (colonne de droite ci-dessous).
                    {move || interaction.get().is_none().then(|| {
                        let send = make_send();
                        let send_on_click = make_send();
                        view! {
                            <div class="flex gap-2 items-center px-3 py-2 border-t border-blue-france-925 flex-shrink-0">
                                <input
                                    type="text"
                                    class="flex-1 shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                           focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                           bg-gray-100 px-3 py-2 outline-none disabled:opacity-50"
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
                                <label class="text-sm font-bold text-gray-900" for=field_id.clone()>
                                    {q_label}
                                </label>
                                {if let Some(opts) = q_opts {
                                    view! {
                                        <select
                                            id=field_id
                                            class="shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                                   focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                                   bg-gray-100 px-3 py-2 outline-none disabled:opacity-50 w-full"
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
                                                   bg-gray-100 px-3 py-2 outline-none disabled:opacity-50 w-full"
                                            prop:value=value
                                            prop:disabled=move || pending.get()
                                            on:input=move |ev| set_value_ev(event_target_value(&ev))
                                        />
                                    }.into_any()
                                }}
                                <label class="flex items-center gap-2 text-sm text-gray-700 cursor-pointer"
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
                                        <label class="text-sm font-bold text-gray-900" for=reason_id.clone()>
                                            "Précisez pourquoi"
                                        </label>
                                        <textarea
                                            id=reason_id.clone()
                                            class="shadow-[inset_0_0_0_1px] shadow-gray-400 \
                                                   focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france \
                                                   bg-gray-100 px-3 py-2 outline-none disabled:opacity-50 \
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
                                       border-l border-blue-france-925"
                                style:width=move || format!("{}px", interaction_panel_width.get())
                            >
                                <p class="text-sm italic text-gray-600">{prompt}</p>
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
            </div>
        </div>
    }
}
