//! En-tête DSFR de l'éditeur d'acte légal.
//!
//! Affiche le [`dsfr::Header`] avec deux zones de boutons :
//! - Les **actions racine** (Visa, Considérant, Sur, Titre, Article, Annexe),
//!   toujours visibles.
//! - Le **portail contextuel** : boutons poussés par les composants enfants dans
//!   [`super::context::EditorContext::portal_actions`].
//! - Le **bouton agent IA** : affiché uniquement si `has_agent` est `true`.

use leptos::prelude::*;

use dsfr::{Button, ButtonGroup, ButtonVariant, Header, SubHeader};

use super::content::is_list_ordered;
use super::context::expect_editor_context;
use super::widgets::{FormatToolbar, TOOLBAR_BTN_CLASS};
use crate::traits::node::BodyAccess;
use crate::{NodeId, NodeKind, NodeSpec};

/// Identité d'un utilisateur connecté à la salle de collaboration, affichée
/// sous forme de pastille dans l'en-tête de l'éditeur (voir `app::ws::
/// RoomHandle::connected_users`, alimenté par `ServerMessage::Presence`).
/// Initiale et couleur sont calculées côté serveur, pour que tous les pairs
/// affichent la même pastille pour un même utilisateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedUser {
    pub user_id: String,
    pub initial: String,
    pub color: String,
}

/// En-tête de l'éditeur : bloc Marianne + boutons d'action + portail contextuel.
#[component]
pub fn EditorHeader(
    /// Afficher le bouton de bascule du panneau agent IA.
    #[prop(optional)]
    has_agent: bool,
    /// Signal d'état du panneau agent (ouvert / fermé).
    agent_panel_open: Signal<bool>,
    /// Callback appelé pour basculer le panneau agent.
    on_toggle_agent: Callback<()>,
    /// Initiale du nom affiché de l'utilisateur courant, pour la bulle
    /// d'avatar menant à `/account` (voir `app::auth::HeaderIdentity`).
    /// `None` masque la bulle (page hôte non authentifiée).
    user_initial: Option<String>,
    /// `true` si l'utilisateur courant a accès au panneau administrateur
    /// (voir `app::auth::HeaderIdentity::is_admin`) : affiche un lien vers
    /// `/admin`.
    #[prop(optional)]
    is_admin: bool,
    /// Autres utilisateurs actuellement connectés à la salle de
    /// collaboration, affichés en pastilles à côté de la bulle de
    /// l'utilisateur courant. Vide (ou absent) tant qu'aucun autre
    /// utilisateur n'est présent.
    #[prop(optional, into)]
    connected_users: Option<Signal<Vec<ConnectedUser>>>,
) -> impl IntoView {
    let ctx = expect_editor_context();
    let root = ctx.body.with_untracked(|b| b.root());

    let has_sur = move || {
        ctx.body.with(|b| {
            b.children_of(root)
                .iter()
                .any(|&id| b.kind_of(id) == NodeKind::Sur)
        })
    };

    let portal_actions = move || ctx.portal_actions.get();

    view! {
        <Header service_title="Redac'Ap" service_tagline="Éditeur d'arrêté préfectoral".to_string()>
            // ── Actions racine ────────────────────────────────────────────
            <SubHeader slot>
                <ButtonGroup class="divide-x">
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| { let _ = b.append_node(root, NodeSpec::Visa); });
                        }
                    >
                        "+ Visa"
                    </Button>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| { let _ = b.append_node(root, NodeSpec::Considerant); });
                        }
                    >
                        "+ Considérant"
                    </Button>
                    <Show when=move || !has_sur()>
                        <Button
                            variant=ButtonVariant::TertiaryNoOutline
                            size=dsfr::Size::Sm
                            on_click=move |_| {
                                ctx.body.update(|b| { let _ = b.append_node(root, NodeSpec::Sur); });
                            }
                        >
                            "+ Sur"
                        </Button>
                    </Show>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| {
                                let _ = b.append_node(root, NodeSpec::Article(Default::default()));
                            });
                        }
                    >
                        "+ Article"
                    </Button>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| {
                                let _ = b.append_node(root, NodeSpec::Titre(Default::default()));
                            });
                        }
                    >
                        "+ Titre"
                    </Button>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| {
                                let _ = b.append_node(root, NodeSpec::Annexe(Default::default()));
                            });
                        }
                    >
                        "+ Annexe"
                    </Button>
                    // ── Bascule panneau agent ─────────────────────────────────────
                    {has_agent.then(|| view! {
                        <Button
                            variant=ButtonVariant::TertiaryNoOutline
                            size=dsfr::Size::Sm
                            on_click=move |_| on_toggle_agent.run(())
                        >
                            {move || if agent_panel_open.get() { "Masquer l'agent" } else { "Agent IA" }}
                        </Button>
                    })}
                </ButtonGroup>

                // ── Outils de mise en forme (nœud contenant un span) ──────
                <Show when=move || ctx.content_focus.get()>
                    <span class="inline-flex items-center align-middle gap-2 ml-2">
                        <span class="w-px h-6 bg-gray-300 inline-block"/>
                        <FormatToolbar/>
                    </span>
                </Show>

                // ── Barre contextuelle de contenu ─────────────────────────
                <ContentToolbar/>

            </SubHeader>



            // ── Portail contextuel ────────────────────────────────────────
            <Show when=move || !portal_actions().is_empty()>
                <div class="w-px h-6 bg-gray-300 shrink-0"/>
                <ButtonGroup>
                    <For
                        each=portal_actions
                        key=|a| a.label.clone()
                        children=|action| {
                            let on_click = action.on_click.clone();
                            view! {
                                <Button
                                    variant=action.variant
                                    size=dsfr::Size::Sm
                                    on_click=move |_| (on_click)()
                                >
                                    {action.label}
                                </Button>
                            }
                        }
                    />
                </ButtonGroup>
            </Show>

            // ── Identité utilisateur ──────────────────────────────────────
            {is_admin.then(|| view! {
                <a
                    href="/admin"
                    class="text-sm font-bold text-blue-france dark:text-blue-france-925 hover:underline whitespace-nowrap"
                >
                    "Administration"
                </a>
            })}
            <ConnectedUserBadges connected_users=connected_users.unwrap_or_else(|| Signal::derive(Vec::new))/>
            {user_initial.map(|initial| view! {
                <a
                    href="/account"
                    title="Mon compte"
                    class="flex items-center justify-center w-9 h-9 rounded-full bg-blue-france text-white font-bold hover:bg-blue-france-hover transition-colors shrink-0"
                >
                    {initial}
                </a>
            })}

        </Header>
    }
}

/// Pastilles des autres utilisateurs connectés à la salle de collaboration,
/// affichées à côté de la bulle de l'utilisateur courant (voir
/// [`EditorHeader`]) : initiale sur fond de couleur déterministe (voir
/// `server::editor::presence::color_for_id`), pour distinguer visuellement
/// plusieurs pairs sans avoir à afficher leur nom complet.
#[component]
fn ConnectedUserBadges(connected_users: Signal<Vec<ConnectedUser>>) -> impl IntoView {
    view! {
        <div class="flex items-center -space-x-2">
            <For
                each=move || connected_users.get()
                key=|user| user.user_id.clone()
                children=|user| view! {
                    <span
                        title=user.initial.clone()
                        class="flex items-center justify-center w-8 h-8 rounded-full text-white \
                               text-sm font-bold border-2 border-white shrink-0"
                        style:background-color=user.color.clone()
                    >
                        {user.initial.clone()}
                    </span>
                }
            />
        </div>
    }
}

// ── Barre contextuelle de contenu ─────────────────────────────────────────────

/// Barre de boutons contextuelle affichée dans le sous-en-tête quand un nœud
/// de contenu a le focus (Paragraphe, ListItem, TableCell). Utilise
/// `mousedown + preventDefault` pour ne pas interrompre le focus du div en
/// cours d'édition (même idiome que [`FormatToolbar`]).
#[component]
fn ContentToolbar() -> impl IntoView {
    let ctx = expect_editor_context();

    move || {
        let node_id = ctx.content_focus_node.get()?;
        let kind = ctx.body.with(|b| b.kind_of(node_id));

        if !matches!(
            kind,
            NodeKind::ListItem | NodeKind::Paragraphe | NodeKind::TableCell
        ) {
            return None;
        }

        Some(view! {
            <span class="inline-flex items-center align-middle gap-2 ml-2">
                <span class="w-px h-6 bg-gray-300 inline-block"/>
                {match kind {
                    NodeKind::ListItem  => view! { <ListContentToolbar item_id=node_id/> }.into_any(),
                    NodeKind::Paragraphe => view! { <ParagraphContentToolbar para_id=node_id/> }.into_any(),
                    NodeKind::TableCell  => view! { <TableContentToolbar cell_id=node_id/> }.into_any(),
                    _ => unreachable!(),
                }}
                <CommentSelectionButton/>
            </span>
        })
    }
}

/// Bouton "Commenter" : capture la sélection navigateur courante dans le
/// nœud de contenu en cours d'édition (voir
/// [`super::selection_dom::capture_content_selection`]) et ouvre le
/// compositeur du panneau Commentaires, pré-rempli avec l'extrait
/// sélectionné le cas échéant. Utilise `mousedown + preventDefault` pour ne
/// pas interrompre le focus du div en cours d'édition (même idiome que
/// [`FormatToolbar`]) : c'est ce qui permet de retrouver l'élément focus via
/// `document().active_element()`.
#[component]
fn CommentSelectionButton() -> impl IntoView {
    let ctx = expect_editor_context();

    view! {
        <button
            type="button"
            title="Commenter la sélection"
            class="text-xs text-blue-france dark:text-blue-france-925 hover:underline cursor-pointer"
            on:mousedown=move |ev| {
                ev.prevent_default();
                let seed = document()
                    .active_element()
                    .and_then(|active| {
                        ctx.body
                            .with_untracked(|b| super::selection_dom::capture_content_selection(&active, b))
                    });
                ctx.side_panel_open.set(true);
                ctx.side_panel_tab.set(1);
                match seed {
                    Some((selection, excerpt)) => {
                        ctx.pending_comment.set(Some(super::state::PendingComment {
                            selection: Some(selection),
                            excerpt: Some(excerpt),
                        }));
                    }
                    None => {
                        ctx.pending_comment.set(Some(super::state::PendingComment::default()));
                    }
                }
            }
        >
            "💬 Commenter"
        </button>
    }
}

#[component]
fn ListContentToolbar(item_id: NodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let list_id = ctx
        .body
        .with_untracked(|b| b.parent_of(item_id).unwrap_or(item_id));
    let is_ordered = Signal::derive(move || ctx.body.with(|b| is_list_ordered(b, list_id)));

    view! {
        <div class="flex items-center gap-1">
            // Bascule type de liste
            <div class="inline-flex border border-gray-300 rounded overflow-hidden text-xs">
                <button
                    title="Liste à puces"
                    class=move || if !is_ordered.get() {
                        "px-2 py-0.5 bg-teal-600 text-white font-medium"
                    } else {
                        "px-2 py-0.5 text-gray-600 hover:bg-teal-50 cursor-pointer"
                    }
                    on:mousedown=move |ev| {
                        ev.prevent_default();
                        if is_ordered.get_untracked() {
                            ctx.body.update(|b| {
                                if let NodeSpec::List(list) = b.spec_of(list_id) {
                                    let _ = b.set_spec_unchecked(list_id, NodeSpec::List(content::List {
                                        marker: content::ListMarker::Disc,
                                        ..list
                                    }));
                                }
                            });
                        }
                    }
                >"•"</button>
                <button
                    title="Liste numérotée"
                    class=move || if is_ordered.get() {
                        "px-2 py-0.5 bg-teal-600 text-white font-medium border-l border-gray-300"
                    } else {
                        "px-2 py-0.5 text-gray-600 hover:bg-teal-50 cursor-pointer border-l border-gray-300"
                    }
                    on:mousedown=move |ev| {
                        ev.prevent_default();
                        if !is_ordered.get_untracked() {
                            ctx.body.update(|b| {
                                if let NodeSpec::List(list) = b.spec_of(list_id) {
                                    let _ = b.set_spec_unchecked(list_id, NodeSpec::List(content::List {
                                        marker: content::ListMarker::Decimal,
                                        ..list
                                    }));
                                }
                            });
                        }
                    }
                >"1."</button>
            </div>
            <button
                class=TOOLBAR_BTN_CLASS
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    let new_id = ctx.body.try_update(|b| -> Option<NodeId> {
                        let item = b.create_node(NodeSpec::ListItem(content::ListItem::default()));
                        let plain = b.create_node(NodeSpec::Plain(String::new()));
                        b.append_child_unchecked(item, plain).ok()?;
                        b.insert_sibling_after(item_id, item).ok()?;
                        Some(item)
                    }).flatten();
                    if let Some(id) = new_id {
                        ctx.request_focus(id, false);
                    }
                }
            >"+ Élément"</button>
            <button
                class="text-xs text-red-500 hover:text-red-700 px-1"
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    ctx.remove_node_with_comments(list_id);
                }
            >"× Liste"</button>
        </div>
    }
}

#[component]
fn ParagraphContentToolbar(para_id: NodeId) -> impl IntoView {
    let ctx = expect_editor_context();
    let parent_kind = ctx
        .body
        .with_untracked(|b| b.parent_of(para_id).map(|p| b.kind_of(p)));
    let can_add_table = parent_kind == Some(NodeKind::ArticleBody);

    view! {
        <div class="flex items-center gap-1">
            <button
                class=TOOLBAR_BTN_CLASS
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    let new_id = ctx.body.try_update(|b| -> Option<NodeId> {
                        let new_para = b.create_node(NodeSpec::Paragraphe);
                        let plain = b.create_node(NodeSpec::Plain(String::new()));
                        b.append_child_unchecked(new_para, plain).ok()?;
                        b.insert_sibling_after(para_id, new_para).ok()?;
                        Some(new_para)
                    }).flatten();
                    if let Some(id) = new_id {
                        ctx.request_focus(id, false);
                    }
                }
            >"+ Paragraphe"</button>
            <button
                class=TOOLBAR_BTN_CLASS
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    let new_id = ctx.body.try_update(|b| -> Option<NodeId> {
                        let list = b.create_node(NodeSpec::List(Default::default()));
                        let item = b.create_node(NodeSpec::ListItem(content::ListItem::default()));
                        let plain = b.create_node(NodeSpec::Plain(String::new()));
                        b.append_child_unchecked(item, plain).ok()?;
                        b.append_child_unchecked(list, item).ok()?;
                        b.insert_sibling_after(para_id, list).ok()?;
                        Some(item)
                    }).flatten();
                    if let Some(id) = new_id {
                        ctx.request_focus(id, false);
                    }
                }
            >"• Liste à puces"</button>
            <button
                class=TOOLBAR_BTN_CLASS
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    let new_id = ctx.body.try_update(|b| -> Option<NodeId> {
                        let list = b.create_node(NodeSpec::List(content::List {
                            marker: content::ListMarker::Decimal,
                            start: None,
                        }));
                        let item = b.create_node(NodeSpec::ListItem(content::ListItem::default()));
                        let plain = b.create_node(NodeSpec::Plain(String::new()));
                        b.append_child_unchecked(item, plain).ok()?;
                        b.append_child_unchecked(list, item).ok()?;
                        b.insert_sibling_after(para_id, list).ok()?;
                        Some(item)
                    }).flatten();
                    if let Some(id) = new_id {
                        ctx.request_focus(id, false);
                    }
                }
            >"1. Liste numérotée"</button>
            {can_add_table.then(|| view! {
                <button
                    class=TOOLBAR_BTN_CLASS
                    on:mousedown=move |ev| {
                        ev.prevent_default();
                        let new_cell = ctx.body.try_update(|b| -> Option<NodeId> {
                            let table = b.create_node(NodeSpec::Table);
                            let row = b.create_node(NodeSpec::TableRow);
                            let cell = b.create_node(NodeSpec::TableCell);
                            let para = b.create_node(NodeSpec::Paragraphe);
                            let plain = b.create_node(NodeSpec::Plain(String::new()));
                            b.append_child_unchecked(para, plain).ok()?;
                            b.append_child_unchecked(cell, para).ok()?;
                            b.append_child_unchecked(row, cell).ok()?;
                            b.append_child_unchecked(table, row).ok()?;
                            b.insert_sibling_after(para_id, table).ok()?;
                            Some(cell)
                        }).flatten();
                        if let Some(id) = new_cell {
                            ctx.request_focus(id, false);
                        }
                    }
                >"+ Tableau"</button>
            })}
        </div>
    }
}

#[component]
fn TableContentToolbar(cell_id: NodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    view! {
        <div class="flex items-center gap-1">
            <button
                class=TOOLBAR_BTN_CLASS
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    let first_cell = ctx.body.try_update(|b| -> Option<NodeId> {
                        let row_id = b.parent_of(cell_id)?;
                        let col_count = b.children_of(row_id).len().max(1);
                        let new_row = b.create_node(NodeSpec::TableRow);
                        let mut first = None;
                        for i in 0..col_count {
                            let cell = b.create_node(NodeSpec::TableCell);
                            let para = b.create_node(NodeSpec::Paragraphe);
                            let plain = b.create_node(NodeSpec::Plain(String::new()));
                            let _ = b.append_child_unchecked(para, plain);
                            let _ = b.append_child_unchecked(cell, para);
                            let _ = b.append_child_unchecked(new_row, cell);
                            if i == 0 { first = Some(cell); }
                        }
                        let _ = b.insert_sibling_after(row_id, new_row);
                        first
                    }).flatten();
                    if let Some(id) = first_cell {
                        ctx.request_focus(id, false);
                    }
                }
            >"+ Ligne"</button>
            <button
                class=TOOLBAR_BTN_CLASS
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    ctx.body.update(|b| {
                        let row_id = b.parent_of(cell_id).unwrap_or(cell_id);
                        let table_id = b.parent_of(row_id).unwrap_or(row_id);
                        let rows = b.children_of(table_id);
                        for r in rows {
                            let cell = b.create_node(NodeSpec::TableCell);
                            let para = b.create_node(NodeSpec::Paragraphe);
                            let plain = b.create_node(NodeSpec::Plain(String::new()));
                            let _ = b.append_child_unchecked(para, plain);
                            let _ = b.append_child_unchecked(cell, para);
                            let _ = b.append_child_unchecked(r, cell);
                        }
                    });
                }
            >"+ Colonne"</button>
            <button
                class="text-xs text-red-500 hover:text-red-700 px-1"
                on:mousedown=move |ev| {
                    ev.prevent_default();
                    let table_id = ctx.body.with_untracked(|b| {
                        let row_id = b.parent_of(cell_id).unwrap_or(cell_id);
                        b.parent_of(row_id).unwrap_or(row_id)
                    });
                    ctx.remove_node_with_comments(table_id);
                }
            >"× Tableau"</button>
        </div>
    }
}
