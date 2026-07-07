use leptos::prelude::*;

use agent::{AgentPanel, InteractionRequest, InteractionResponse, PanelEntry};
use dsfr::{BlocMarianneInline, ResizeHandle, TabPanel, Tabs};

use super::content::ContentSubtree;
use super::context::{expect_editor_context, provide_editor_context};
use super::header::{ConnectedUser, EditorHeader};
use super::review::ReviewPanel;
use super::widgets::{InlineEditableDiv, TOOLBAR_BTN_CLASS};
use crate::traits::node::{BodyRead, BodyWrite};
use crate::{Body, BodyNodeId, NodeKind, NodeSpec, Review};

/// Largeur initiale du panneau agent IA, en pixels (équivalente à l'ancienne
/// classe Tailwind fixe `w-80`).
const AGENT_PANEL_DEFAULT_WIDTH: f64 = 320.0;
/// Bornes de largeur autorisées lors du redimensionnement par glissement
/// (voir [`ResizeHandle`]) : assez étroit pour laisser de la place au
/// contenu principal, assez large pour rester lisible.
const AGENT_PANEL_MIN_WIDTH: f64 = 240.0;
const AGENT_PANEL_MAX_WIDTH: f64 = 640.0;

/// Point d'entrée de l'éditeur d'acte légal.
/// Fournit le [`EditorContext`](super::context::EditorContext) et affiche le corps.
/// Accepte un [`Body::Direct`] ou un [`Body::Yrs`], possédé par la page hôte
/// (composant contrôlé) : elle peut ainsi continuer à écrire dans `body`
/// après le montage, par exemple pour y appliquer des mises à jour Yrs
/// reçues d'un salon de collaboration (voir `app::ws`).
///
/// Le panneau agent IA est affiché en barre latérale droite lorsque les trois
/// props `agent_messages`, `agent_pending` et `on_agent_send` sont fournis.
/// La page hôte reste responsable de l'appel réel à l'agent et de la mise à
/// jour de ces signaux en retour. `agent_interaction`/`on_agent_respond`
/// relaient de la même façon le formulaire d'interaction de [`AgentPanel`].
#[component]
pub fn LegalActEditor(
    body: RwSignal<Body>,
    /// Commentaires et notes de travail du projet (voir [`crate::Review`]),
    /// possédés par la page hôte au même titre que `body`, notamment pour
    /// pouvoir y appliquer des mises à jour Yrs reçues d'un salon de
    /// collaboration. Un [`Review::direct`] local est créé si absent.
    #[prop(optional)]
    reviews: Option<RwSignal<Review>>,
    /// Identité affichée de l'utilisateur courant : auteur des commentaires
    /// qu'il crée. `None` (page hôte non authentifiée) masque la création
    /// de commentaires.
    current_user: Option<String>,
    /// `true` si l'utilisateur courant a les droits d'édition sur ce projet
    /// (voir exigence : un commentaire peut être résolu par son auteur ou
    /// par un rédacteur).
    #[prop(optional)]
    can_edit: bool,
    /// Autorité signataire (ex. « PRÉFET DE LA RÉGION ÎLE-DE-FRANCE »),
    /// affichée dans le bloc-marque en en-tête de la première page. `None`
    /// tant que l'acte n'a pas d'autorité résolue (voir
    /// [`crate::traits::act::LegalActMeta::autorite_id`]) : le bloc-marque
    /// n'est alors pas affiché plutôt que d'inventer un texte.
    #[prop(optional, into)]
    autorite: Option<String>,
    /// Historique des échanges avec l'agent IA (messages, réflexions,
    /// traces d'appels d'outils, voir [`agent::PanelEntry`]).
    #[prop(optional, into)]
    agent_messages: Option<Signal<Vec<PanelEntry>>>,
    /// `true` tant que l'agent n'a pas renvoyé sa réponse finale.
    #[prop(optional, into)]
    agent_pending: Option<Signal<bool>>,
    /// Appelé avec le texte saisi lorsque l'utilisateur envoie un message.
    #[prop(optional)]
    on_agent_send: Option<Callback<String>>,
    /// Formulaire d'interaction à afficher (voir [`AgentPanel`]'s `interaction`).
    #[prop(optional, into)]
    agent_interaction: Option<Signal<Option<InteractionRequest>>>,
    /// Appelé avec les réponses de l'utilisateur au formulaire d'interaction.
    #[prop(optional)]
    on_agent_respond: Option<Callback<InteractionResponse>>,
    /// `true` si l'utilisateur a choisi d'accepter automatiquement toutes
    /// les modifications proposées par l'agent (voir [`AgentPanel`]'s
    /// `auto_accept`).
    #[prop(optional, into)]
    agent_auto_accept: Option<Signal<bool>>,
    /// Appelé avec la nouvelle valeur lorsque l'utilisateur bascule la case
    /// « accepter toutes les modifications ».
    #[prop(optional)]
    on_agent_toggle_auto_accept: Option<Callback<bool>>,
    /// Appelé lorsque l'utilisateur choisit d'effacer l'historique de la
    /// conversation avec l'agent (voir [`AgentPanel`]'s `on_clear_history`).
    #[prop(optional)]
    on_agent_clear_history: Option<Callback<()>>,
    /// Appelé à chaque changement du nœud ciblé pour l'agent IA (bouton
    /// « Cibler », voir [`super::context::EditorContext::agent_target`]),
    /// pour que la page hôte le transmette au serveur (voir `app::ws::
    /// RoomHandle::set_selection`). L'agent peut alors viser ce nœud via le
    /// mot-clé `"selection"`, sans que l'utilisateur ait à en connaître
    /// l'identifiant technique.
    #[prop(optional)]
    on_agent_target: Option<Callback<Option<BodyNodeId>>>,
    /// Initiale du nom affiché de l'utilisateur courant, transmise telle
    /// quelle à [`EditorHeader`] pour la bulle d'avatar menant à `/account`.
    /// `None` la masque (page hôte non authentifiée).
    user_initial: Option<String>,
    /// Transmis tel quel à [`EditorHeader`] : affiche un lien vers `/admin`
    /// si l'utilisateur courant a accès au panneau administrateur.
    #[prop(optional)]
    is_admin: bool,
    /// Transmis tel quel à [`EditorHeader`] : autres utilisateurs
    /// actuellement connectés à la salle de collaboration, affichés en
    /// pastilles à côté de la bulle de l'utilisateur courant.
    #[prop(optional, into)]
    connected_users: Option<Signal<Vec<ConnectedUser>>>,
    /// Contenu de l'onglet « Paramètres » du panneau latéral (ex. gestion
    /// des intentions rattachées au projet, voir
    /// `app::pages::project_intentions::ProjectIntentionsPanel`) : ce
    /// composant reste agnostique de ce contenu, fourni par la page hôte.
    #[prop(optional)]
    children: Option<ChildrenFn>,
) -> impl IntoView {
    let reviews = reviews.unwrap_or_else(|| RwSignal::new(Review::direct()));
    let ctx = provide_editor_context(body, reviews, current_user, can_edit);
    if let Some(on_agent_target) = on_agent_target {
        Effect::new(move |_| on_agent_target.run(ctx.agent_target.get()));
    }

    let agent_cfg = match (agent_messages, agent_pending, on_agent_send) {
        (Some(msgs), Some(pending), Some(on_send)) => Some((msgs, pending, on_send)),
        _ => None,
    };
    let agent_interaction = agent_interaction.unwrap_or_else(|| Signal::derive(|| None));
    let connected_users = connected_users.unwrap_or_else(|| Signal::derive(Vec::new));
    let on_agent_respond = on_agent_respond.unwrap_or_else(|| Callback::new(|_| {}));
    let agent_auto_accept = agent_auto_accept.unwrap_or_else(|| Signal::derive(|| false));
    let on_agent_toggle_auto_accept =
        on_agent_toggle_auto_accept.unwrap_or_else(|| Callback::new(|_| {}));
    let on_agent_clear_history = on_agent_clear_history.unwrap_or_else(|| Callback::new(|_| {}));

    // `StoredValue` (Copy) plutôt qu'une capture directe de `children` (non
    // `Copy`, `Rc<dyn Fn...>`) : le bloc réactif ci-dessous s'exécute à
    // chaque bascule de `show_agent`, ce qui exige des captures `Copy`.
    let children = StoredValue::new(children);

    let has_agent = agent_cfg.is_some();
    let show_agent = ctx.side_panel_open;
    let agent_panel_width = RwSignal::new(AGENT_PANEL_DEFAULT_WIDTH);
    let agent_panel_open = Signal::from(show_agent.read_only());
    let on_toggle_agent = Callback::new(move |()| {
        if has_agent {
            show_agent.update(|v| *v = !*v);
        }
    });

    let selected_tab = ctx.side_panel_tab;

    view! {
        <div class="legal-act-editor flex flex-col min-h-screen max-h-screen text-base leading-relaxed">
            <div class="no-print">
                <EditorHeader
                    has_agent=has_agent
                    agent_panel_open=agent_panel_open
                    on_toggle_agent=on_toggle_agent
                    user_initial=user_initial
                    is_admin=is_admin
                    connected_users=connected_users
                />
            </div>
            <div class="flex flex-1 overflow-hidden">
                <main class="flex-1 overflow-y-auto bg-gray-200 dark:bg-gray-800 print:bg-white print:overflow-visible">
                    // Le papier reste blanc en mode sombre (métaphore de la
                    // feuille imprimée) : seul le plateau qui l'entoure
                    // bascule en sombre. `text-gray-900` fixe la couleur du
                    // texte indépendamment du thème : sans cela, il hérite
                    // de `dark:text-gray-100` posé sur `<main>` (voir
                    // `app::app::App`) et devient illisible sur fond blanc.
                    <div class="legal-act-page my-4 mx-auto bg-white text-gray-900">
                        {autorite.map(|a| view! {
                            <BlocMarianneInline autorite=a class="mb-8"/>
                        })}
                        <EditActTitle/>
                        <EditBody/>
                    </div>
                </main>
                {move || show_agent.get().then(|| {
                    view! {
                        <div class="no-print contents">
                            <ResizeHandle
                                width=agent_panel_width
                                min_width=AGENT_PANEL_MIN_WIDTH
                                max_width=AGENT_PANEL_MAX_WIDTH
                            />
                            <aside class="shrink-0 overflow-hidden flex flex-col min-h-0" style:width=move || format!("{}px", agent_panel_width.get())>
                                <Tabs titles=vec!["Marie", "Commentaires", "Paramètres"] selected=selected_tab></Tabs>
                                <TabPanel index=0 selected=selected_tab class="flex-1 flex flex-col min-h-0">
                                    {has_agent.then(|| {
                                        let (msgs, pending, on_send) = agent_cfg.expect("has_agent est vrai");
                                        view! {
                                            <AgentTargetIndicator/>
                                            <AgentPanel
                                                messages=msgs
                                                pending=pending
                                                on_send=move |text: String| on_send.run(text)
                                                interaction=agent_interaction
                                                on_respond=on_agent_respond
                                                auto_accept=agent_auto_accept
                                                on_toggle_auto_accept=on_agent_toggle_auto_accept
                                                on_clear_history=on_agent_clear_history
                                            />
                                        }.into_any()
                                    })}
                                </TabPanel>
                                <TabPanel index=1 selected=selected_tab class="flex-1 flex flex-col min-h-0 overflow-y-auto">
                                    <ReviewPanel/>
                                </TabPanel>
                                <TabPanel index=2 selected=selected_tab class="flex-1 flex flex-col min-h-0 overflow-y-auto">
                                    {children.get_value().map(|children| children())}
                                </TabPanel>
                            </aside>
                        </div>
                    }
                })}
            </div>
        </div>
    }
}

// ── Titre de l'acte ──────────────────────────────────────────────────────────

/// Titre de l'acte (ex. « Arrêté préfectoral portant autorisation
/// d'exploiter... »), édité en place en tête du document. Distinct des
/// nœuds `Titre` du corps (subdivisions numérotées « Titre I », « Titre
/// II »...), c'est une propriété du document dans son ensemble portée
/// directement par [`Body::title`]/[`Body::set_title`] plutôt que par un
/// nœud (voir [`crate::traits::node::BodyRead::title`]).
#[component]
fn EditActTitle() -> impl IntoView {
    let ctx = expect_editor_context();
    let title = Signal::derive(move || ctx.body.with(|b| b.title()));

    view! {
        <div class="text-center font-bold text-lg my-6 uppercase tracking-wide">
            <InlineEditableDiv
                text=title
                on_save=move |s| ctx.body.update(|b| b.set_title(&s))
                prevent_newlines=true
            />
        </div>
    }
}

// ── Corps ────────────────────────────────────────────────────────────────────

#[component]
fn EditBody() -> impl IntoView {
    let ctx = expect_editor_context();
    let root = ctx.body.with_untracked(|b| b.root());

    view! {
        <div class="space-y-1">
            // ── Visas ──────────────────────────────────────────────────────
            <section class="mb-4">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(root).into_iter()
                            .filter(|&id| b.kind_of(id) == NodeKind::Visa)
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditVisa node_id=id/> }
                />
            </section>

            // ── Considérants ───────────────────────────────────────────────
            <section class="mb-4">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(root).into_iter()
                            .filter(|&id| b.kind_of(id) == NodeKind::Considerant)
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditConsiderant node_id=id/> }
                />
            </section>

            // ── Sur ────────────────────────────────────────────────────────
            {move || {
                ctx.body.with(|b| {
                    b.children_of(root).into_iter()
                        .find(|&id| b.kind_of(id) == NodeKind::Sur)
                }).map(|id| view! { <EditSur node_id=id/> })
            }}

            // ── ARRÊTE ─────────────────────────────────────────────────────
            <div class="text-center font-bold text-lg my-8 tracking-widest border-y border-gray-300 py-3">
                "ARRÊTE"
            </div>

            // ── Dispositif ─────────────────────────────────────────────────
            <section class="mb-4">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(root).into_iter()
                            .filter(|&id| matches!(
                                b.kind_of(id),
                                NodeKind::Titre | NodeKind::Section
                                    | NodeKind::Chapitre | NodeKind::Article
                            ))
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditStructuralNode node_id=id/> }
                />
            </section>

            // ── Annexes ────────────────────────────────────────────────────
            <Show when=move || ctx.body.with(|b| {
                b.children_of(root).iter().any(|&id| b.kind_of(id) == NodeKind::Annexe)
            })>
                <section class="mt-8 pt-4 border-t border-gray-300">
                    <div class="text-center font-bold text-sm tracking-widest uppercase mb-4">
                        "Annexes"
                    </div>
                    <For
                        each=move || ctx.body.with(|b| {
                            b.children_of(root).into_iter()
                                .filter(|&id| b.kind_of(id) == NodeKind::Annexe)
                                .collect::<Vec<_>>()
                        })
                        key=|id| *id
                        children=|id| view! { <EditAnnexe node_id=id/> }
                    />
                </section>
            </Show>
        </div>
    }
}

// ── Ciblage pour l'agent IA ──────────────────────────────────────────────────

/// Bandeau affiché au-dessus du panneau agent quand un nœud est ciblé (voir
/// [`TargetButton`]) : rappelle à l'utilisateur ce que l'agent visera s'il
/// utilise « ce considérant »/« cet article »... dans sa demande, en des
/// termes qu'il comprend (type + numéro), jamais l'identifiant technique.
#[component]
fn AgentTargetIndicator() -> impl IntoView {
    let ctx = expect_editor_context();
    let label = move || {
        ctx.agent_target.get().map(|id| {
            ctx.body.with(|b| {
                let kind = b.kind_of(id);
                match b.spec_of(id).number() {
                    Some(n) => format!("{kind} {n}"),
                    None => kind.to_string(),
                }
            })
        })
    };

    view! {
        {move || label().map(|text| view! {
            <div class="flex items-center justify-between gap-2 px-3 py-1.5 text-xs \
                        bg-blue-france-975 dark:bg-gray-800 text-blue-france dark:text-blue-france-925 border-b border-blue-france-925 dark:border-gray-700">
                <span>"Cible pour l'agent IA : " <strong>{text}</strong></span>
                <button
                    type="button"
                    class="text-blue-france dark:text-blue-france-925 hover:underline cursor-pointer shrink-0"
                    on:click=move |_| ctx.agent_target.set(None)
                >"Retirer"</button>
            </div>
        })}
    }
}

/// Supprime `node_id`, et retire le ciblage agent s'il pointait dessus (voir
/// [`super::context::EditorContext::agent_target`]) — évite qu'une cible
/// reste accrochée à un nœud qui n'existe plus.
fn remove_targetable_node(ctx: super::context::EditorContext, node_id: BodyNodeId) {
    ctx.remove_node_with_comments(node_id);
    if ctx.agent_target.get_untracked() == Some(node_id) {
        ctx.agent_target.set(None);
    }
}

/// Bouton « Cibler » : désigne `node_id` comme la cible de l'agent IA (voir
/// [`super::context::EditorContext::agent_target`]), pour que l'utilisateur
/// puisse dire « complète ce considérant » sans jamais avoir à connaître ou
/// saisir l'identifiant technique du nœud. Un second clic retire le
/// ciblage.
#[component]
fn TargetButton(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let active = move || ctx.agent_target.get() == Some(node_id);

    view! {
        <button
            type="button"
            title="Cibler ce nœud pour l'agent IA"
            class="no-print text-xs shrink-0 transition-colors"
            class:opacity-0=move || !active()
            class:group-hover:opacity-100=move || !active()
            class:text-blue-france=active
            class:dark:text-blue-france-925=active
            class:text-gray-400=move || !active()
            class:hover:text-blue-france=move || !active()
            class:dark:hover:text-blue-france-925=move || !active()
            on:click=move |_| ctx.toggle_agent_target(node_id)
        >
            {move || if active() { "Ciblé ✓" } else { "Cibler" }}
        </button>
    }
}

// ── Nœuds textuels (Visa / Considérant / Sur) ────────────────────────────────

fn plain_text_signal(node_id: BodyNodeId) -> (Option<BodyNodeId>, Signal<String>) {
    let ctx = expect_editor_context();
    let plain_id = ctx.body.with_untracked(|b| b.first_child_of(node_id));
    let text = Signal::derive(move || {
        plain_id
            .map(|pid| ctx.body.with(|b| b.text_of(pid)))
            .unwrap_or_default()
    });
    (plain_id, text)
}

pub(super) fn save_plain_text(plain_id: Option<BodyNodeId>, new_text: String) {
    let ctx = expect_editor_context();
    if let Some(pid) = plain_id {
        ctx.body.update(|b| {
            let len = b.len_of(pid);
            b.remove_text_unchecked(pid, 0, len);
            b.insert_text_unchecked(pid, 0, &new_text);
        });
    }
}

#[component]
fn EditVisa(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let (plain_id, text) = plain_text_signal(node_id);

    view! {
        <div class="group flex items-start gap-3 py-1 hover:bg-gray-50 rounded px-1 transition-colors">
            <span class="font-semibold shrink-0 text-xs text-gray-500 uppercase mt-px">"VU"</span>
            <div class="flex-1 text-sm">
                <InlineEditableDiv
                    text=text
                    on_save=move |s| save_plain_text(plain_id, s)
                />
            </div>
            <TargetButton node_id=node_id/>
            <button
                class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs shrink-0"
                on:click=move |_| remove_targetable_node(ctx, node_id)
            >"×"</button>
        </div>
    }
}

#[component]
fn EditConsiderant(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let (plain_id, text) = plain_text_signal(node_id);

    view! {
        <div class="group flex items-start gap-3 py-1 hover:bg-gray-50 rounded px-1 transition-colors">
            <span class="font-semibold shrink-0 text-xs text-gray-500 uppercase mt-px">"CONSIDÉRANT QUE"</span>
            <div class="flex-1 text-sm">
                <InlineEditableDiv
                    text=text
                    on_save=move |s| save_plain_text(plain_id, s)
                />
            </div>
            <TargetButton node_id=node_id/>
            <button
                class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs shrink-0"
                on:click=move |_| remove_targetable_node(ctx, node_id)
            >"×"</button>
        </div>
    }
}

#[component]
fn EditSur(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let (plain_id, text) = plain_text_signal(node_id);

    view! {
        <div class="group flex items-start gap-3 py-1 hover:bg-gray-50 rounded px-1 transition-colors">
            <span class="font-semibold shrink-0 text-xs text-gray-500 uppercase mt-px">"SUR"</span>
            <div class="flex-1 text-sm">
                <InlineEditableDiv
                    text=text
                    on_save=move |s| save_plain_text(plain_id, s)
                />
            </div>
            <TargetButton node_id=node_id/>
            <button
                class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs shrink-0"
                on:click=move |_| remove_targetable_node(ctx, node_id)
            >"×"</button>
        </div>
    }
}

// ── Nœuds structurels ────────────────────────────────────────────────────────

/// Dispatcher : redirige vers le composant d'édition selon le type du nœud.
#[component]
pub fn EditStructuralNode(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let kind = ctx.body.with_untracked(|b| b.kind_of(node_id));
    match kind {
        NodeKind::Titre => view! { <EditTitre node_id=node_id/> }.into_any(),
        NodeKind::Section => view! { <EditSection node_id=node_id/> }.into_any(),
        NodeKind::Chapitre => view! { <EditChapitre node_id=node_id/> }.into_any(),
        NodeKind::Article => view! { <EditArticle node_id=node_id/> }.into_any(),
        _ => view! { <div/> }.into_any(),
    }
}

#[component]
fn EditTitre(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let number = move || ctx.body.with(|b| b.spec_of(node_id).number().unwrap_or(1));
    let label_id = move || {
        ctx.body.with(|b| {
            b.children_of(node_id)
                .into_iter()
                .find(|&id| b.kind_of(id) == NodeKind::LibelleTitre)
        })
    };

    view! {
        <div class="my-6 group">
            <div class="flex items-center justify-between mb-2">
                <div class="font-bold text-sm tracking-widest uppercase text-gray-700">
                    {move || format!("Titre {}", number())}
                </div>
                {move || label_id().map(|lid| view! { <EditLabel node_id=lid/> })}
                <TargetButton node_id=node_id/>
                <button
                    class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| remove_targetable_node(ctx, node_id)
                >"×"</button>
            </div>
            <div class="ml-4 space-y-2">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(node_id).into_iter()
                            .filter(|&id| !b.kind_of(id).is_label())
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditStructuralNode node_id=id/> }
                />
            </div>
            <div class="flex gap-2 mt-2">
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Chapitre(Default::default())); });
                }>"+ Chapitre"</button>
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Section(Default::default())); });
                }>"+ Section"</button>
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Article(Default::default())); });
                }>"+ Article"</button>
            </div>
        </div>
    }
}

#[component]
fn EditChapitre(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let number = move || ctx.body.with(|b| b.spec_of(node_id).number().unwrap_or(1));
    let label_id = move || {
        ctx.body.with(|b| {
            b.children_of(node_id)
                .into_iter()
                .find(|&id| b.kind_of(id) == NodeKind::LibelleChapitre)
        })
    };

    view! {
        <div class="my-5 group">
            <div class="flex items-center justify-between mb-1">
                <div class="font-semibold text-sm tracking-wide uppercase text-gray-700">
                    {move || format!("Chapitre {}", number())}
                </div>
                {move || label_id().map(|lid| view! { <EditLabel node_id=lid/> })}
                <TargetButton node_id=node_id/>
                <button
                    class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| remove_targetable_node(ctx, node_id)
                >"×"</button>
            </div>
            <div class="ml-4 space-y-2">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(node_id).into_iter()
                            .filter(|&id| !b.kind_of(id).is_label())
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditStructuralNode node_id=id/> }
                />
            </div>
            <div class="flex gap-2 mt-2">
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Section(Default::default())); });
                }>"+ Section"</button>
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Article(Default::default())); });
                }>"+ Article"</button>
            </div>
        </div>
    }
}

#[component]
fn EditSection(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let number = move || ctx.body.with(|b| b.spec_of(node_id).number().unwrap_or(1));
    let label_id = move || {
        ctx.body.with(|b| {
            b.children_of(node_id)
                .into_iter()
                .find(|&id| b.kind_of(id) == NodeKind::LibelleSection)
        })
    };

    view! {
        <div class="my-4 group">
            <div class="flex items-center justify-between mb-1">
                <div class="font-medium text-sm tracking-wide text-gray-700">
                    {move || format!("Section {}", number())}
                </div>
                {move || label_id().map(|lid| view! { <EditLabel node_id=lid/> })}
                <TargetButton node_id=node_id/>
                <button
                    class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| remove_targetable_node(ctx, node_id)
                >"×"</button>
            </div>

            <div class="ml-4">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(node_id).into_iter()
                            .filter(|&id| !b.kind_of(id).is_label())
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditStructuralNode node_id=id/> }
                />
            </div>
            <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Article(Default::default())); });
            }>"+ Article"</button>
        </div>
    }
}

#[component]
fn EditArticle(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let number = move || ctx.body.with(|b| b.spec_of(node_id).number().unwrap_or(1));
    let label_id = move || {
        ctx.body.with(|b| {
            b.children_of(node_id)
                .into_iter()
                .find(|&id| b.kind_of(id) == NodeKind::LibelleArticle)
        })
    };
    let body_id = move || {
        ctx.body.with(|b| {
            b.children_of(node_id)
                .into_iter()
                .find(|&id| b.kind_of(id) == NodeKind::ArticleBody)
        })
    };

    view! {
        <div class="my-4 group border-l-2 border-gray-200 pl-4">
            <div class="flex items-baseline gap-2 mb-1">
                <span class="font-bold text-sm uppercase tracking-wide shrink-0">
                    {move || format!("Article {}", number())}
                </span>
                {move || label_id().map(|lid| view! { <EditLabel node_id=lid/> })}
                <TargetButton node_id=node_id/>
                <button
                    class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs ml-auto shrink-0"
                    on:click=move |_| remove_targetable_node(ctx, node_id)
                >"×"</button>
            </div>
            {move || body_id().map(|bid| view! { <ContentSubtree node_id=bid/> })}
        </div>
    }
}

#[component]
fn EditAnnexe(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let number = move || ctx.body.with(|b| b.spec_of(node_id).number().unwrap_or(1));
    let label_id = move || {
        ctx.body.with(|b| {
            b.children_of(node_id)
                .into_iter()
                .find(|&id| b.kind_of(id) == NodeKind::LibelleAnnexe)
        })
    };

    view! {
        <div class="my-6 group border border-gray-200 rounded p-4">
            <div class="flex items-center justify-between mb-2">
                <div class="font-bold text-sm uppercase tracking-widest text-gray-700">
                    {move || format!("Annexe {}", number())}
                </div>
                {move || label_id().map(|lid| view! { <EditLabel node_id=lid/> })}
                <TargetButton node_id=node_id/>
                <button
                    class="no-print opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| remove_targetable_node(ctx, node_id)
                >"×"</button>
            </div>

            <div class="space-y-2">
                <For
                    each=move || ctx.body.with(|b| {
                        b.children_of(node_id).into_iter()
                            .filter(|&id| !b.kind_of(id).is_label())
                            .collect::<Vec<_>>()
                    })
                    key=|id| *id
                    children=|id| view! { <EditStructuralNode node_id=id/> }
                />
            </div>
            <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Article(Default::default())); });
            }>"+ Article"</button>
        </div>
    }
}

// ── Libellé ──────────────────────────────────────────────────────────────────

/// Édition inline du premier enfant `Plain` d'un nœud `Libellé*`.
#[component]
pub fn EditLabel(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let plain_id = ctx.body.with_untracked(|b| b.first_child_of(node_id));

    let text = Signal::derive(move || {
        plain_id
            .map(|pid| ctx.body.with(|b| b.text_of(pid)))
            .unwrap_or_default()
    });

    view! {
        <InlineEditableDiv
            class="flex-1 mx-4 font-semibold text-sm tracking-wide uppercase"
            text=text
            on_save=move |s| save_plain_text(plain_id, s)
            prevent_newlines=true
        />
    }
}
