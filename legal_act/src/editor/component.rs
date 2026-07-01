use leptos::prelude::*;

use agent::{AgentPanel, PanelMessage};

use crate::{Body, BodyNodeId, NodeKind, NodeSpec};
use crate::traits::node::{BodyRead, BodyWrite};
use super::context::{expect_editor_context, provide_editor_context};
use super::content::ContentSubtree;
use super::header::EditorHeader;
use super::widgets::{InlineEditableDiv, TOOLBAR_BTN_CLASS};

/// Point d'entrée de l'éditeur d'acte légal.
/// Fournit le [`EditorContext`](super::context::EditorContext) et affiche le corps.
/// Accepte un [`Body::Direct`] ou un [`Body::Yrs`] (via `.into()`).
///
/// Le panneau agent IA est affiché en barre latérale droite lorsque les trois
/// props `agent_messages`, `agent_pending` et `on_agent_send` sont fournis.
/// La page hôte reste responsable de l'appel réel à l'agent et de la mise à
/// jour de ces signaux en retour.
#[component]
pub fn LegalActEditor(
    body: Body,
    /// Titre de l'arrêté affiché dans l'en-tête DSFR.
    #[prop(optional, into)]
    title: Option<String>,
    /// Historique des messages échangés avec l'agent IA.
    #[prop(optional, into)]
    agent_messages: Option<Signal<Vec<PanelMessage>>>,
    /// `true` tant que l'agent n'a pas renvoyé sa réponse finale.
    #[prop(optional, into)]
    agent_pending: Option<Signal<bool>>,
    /// Appelé avec le texte saisi lorsque l'utilisateur envoie un message.
    #[prop(optional)]
    on_agent_send: Option<Callback<String>>,
) -> impl IntoView {
    provide_editor_context(body);

    let agent_cfg = match (agent_messages, agent_pending, on_agent_send) {
        (Some(msgs), Some(pending), Some(on_send)) => Some((msgs, pending, on_send)),
        _ => None,
    };

    let has_agent = agent_cfg.is_some();
    let show_agent = RwSignal::new(true);
    let agent_panel_open = Signal::from(show_agent.read_only());
    let on_toggle_agent = Callback::new(move |()| {
        if has_agent {
            show_agent.update(|v| *v = !*v);
        }
    });

    view! {
        <div class="legal-act-editor flex flex-col min-h-screen text-base leading-relaxed">
            <EditorHeader
                title=title.unwrap_or_default()
                has_agent=has_agent
                agent_panel_open=agent_panel_open
                on_toggle_agent=on_toggle_agent
            />
            <div class="flex flex-1 overflow-hidden">
                <main class="flex-1 overflow-y-auto">
                    <div class="max-w-4xl mx-auto py-8 px-6">
                        <EditBody/>
                    </div>
                </main>
                {move || (show_agent.get() && has_agent).then(|| {
                    let (msgs, pending, on_send) = agent_cfg.expect("has_agent est vrai");
                    view! {
                        <aside class="w-80 shrink-0 border-l border-gray-200 overflow-hidden flex flex-col">
                            <AgentPanel
                                messages=msgs
                                pending=pending
                                on_send=move |text: String| on_send.run(text)
                            />
                        </aside>
                    }
                })}
            </div>
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
            <button
                class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs shrink-0"
                on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
            <button
                class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs shrink-0"
                on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
            <button
                class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs shrink-0"
                on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
    let label_id = move || ctx.body.with(|b| {
        b.children_of(node_id).into_iter()
            .find(|&id| b.kind_of(id) == NodeKind::LibelleTitre)
    });

    view! {
        <div class="my-6 group">
            <div class="flex items-center justify-between mb-2">
                <div class="font-bold text-sm tracking-widest uppercase text-gray-700">
                    {move || format!("Titre {}", number())}
                </div>
                {move || label_id().map(|lid| view! {
                    <EditLabel node_id=lid/>
                })}
                <button
                    class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
    let label_id = move || ctx.body.with(|b| {
        b.children_of(node_id).into_iter()
            .find(|&id| b.kind_of(id) == NodeKind::LibelleChapitre)
    });

    view! {
        <div class="my-5 group">
            <div class="flex items-center justify-between mb-1">
                <div class="font-semibold text-sm tracking-wide uppercase text-gray-700">
                    {move || format!("Chapitre {}", number())}
                </div>
                {move || label_id().map(|lid| view! {
                    <EditLabel node_id=lid/>
                })}
                <button
                    class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
    let label_id = move || ctx.body.with(|b| {
        b.children_of(node_id).into_iter()
            .find(|&id| b.kind_of(id) == NodeKind::LibelleSection)
    });

    view! {
        <div class="my-4 group">
            <div class="flex items-center justify-between mb-1">
                <div class="font-medium text-sm tracking-wide text-gray-700">
                    {move || format!("Section {}", number())}
                </div>
                {move || label_id().map(|lid| view! {
                    <EditLabel node_id=lid/>
                })}
                <button
                    class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
    let label_id = move || ctx.body.with(|b| {
        b.children_of(node_id).into_iter()
            .find(|&id| b.kind_of(id) == NodeKind::LibelleArticle)
    });

    view! {
        <div class="my-4 group border-l-2 border-gray-200 pl-4">
            <div class="flex items-baseline gap-2 mb-1">
                <span class="font-bold text-sm uppercase tracking-wide shrink-0">
                    {move || format!("Article {}", number())}
                </span>
                {move || label_id().map(|lid| view! { <EditLabel node_id=lid/> })}
                <button
                    class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs ml-auto shrink-0"
                    on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
                >"×"</button>
            </div>
            <ContentSubtree node_id=node_id/>
            <div class="flex gap-2 mt-2">
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Paragraphe); });
                }>"+ Paragraphe"</button>
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| {
                        let _ = b.append_node(node_id, NodeSpec::List(Default::default()));
                    });
                }>"+ Liste"</button>
                <button class=TOOLBAR_BTN_CLASS on:click=move |_| {
                    ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::Table); });
                }>"+ Tableau"</button>
            </div>
        </div>
    }
}

#[component]
fn EditAnnexe(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let number = move || ctx.body.with(|b| b.spec_of(node_id).number().unwrap_or(1));
    let label_id = move || ctx.body.with(|b| {
        b.children_of(node_id).into_iter()
            .find(|&id| b.kind_of(id) == NodeKind::LibelleAnnexe)
    });

    view! {
        <div class="my-6 group border border-gray-200 rounded p-4">
            <div class="flex items-center justify-between mb-2">
                <div class="font-bold text-sm uppercase tracking-widest text-gray-700">
                    {move || format!("Annexe {}", number())}
                </div>
                {move || label_id().map(|lid| view! {
                    <div class="flex-1 text-left font-medium text-sm mb-2 px-2">
                        <EditLabel node_id=lid/>
                    </div>
                })}
                <button
                    class="opacity-0 group-hover:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| { ctx.body.update(|b| { let _ = b.remove_node(node_id); }); }
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
            class="flex-1 px-2"
            text=text
            on_save=move |s| save_plain_text(plain_id, s)
            prevent_newlines=true
        />
    }
}
