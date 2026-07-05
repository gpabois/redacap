use leptos::prelude::*;

use super::context::expect_editor_context;
use super::widgets::RichEditableDiv;
use crate::traits::node::{BodyRead, BodyWrite};
use crate::{Body, BodyNodeId, NodeKind, NodeSpec};

// ─── Sérialisation HTML ───────────────────────────────────────────────────────

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn node_to_inline_html(body: &impl BodyRead, id: BodyNodeId) -> String {
    match body.kind_of(id) {
        NodeKind::Plain => html_escape(&body.text_of(id)),
        NodeKind::Span => {
            let inner: String = body
                .children_of(id)
                .into_iter()
                .map(|c| node_to_inline_html(body, c))
                .collect();
            if let NodeSpec::Span(span) = body.spec_of(id) {
                let mut s = inner;
                if span.strikeout {
                    s = format!("<s>{s}</s>");
                }
                if span.underline {
                    s = format!("<u>{s}</u>");
                }
                if span.italic {
                    s = format!("<em>{s}</em>");
                }
                if span.bold {
                    s = format!("<strong>{s}</strong>");
                }
                s
            } else {
                inner
            }
        }
        _ => String::new(),
    }
}

fn build_inline_html(body: &impl BodyRead, node_id: BodyNodeId) -> String {
    body.children_of(node_id)
        .into_iter()
        .map(|id| node_to_inline_html(body, id))
        .collect()
}

// ─── Désérialisation HTML ─────────────────────────────────────────────────────

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{00A0}")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// Renvoie la position (dans `html`) du `</{tag_name}>` correspondant à la
/// balise ouvrante qui précède immédiatement cette chaîne (depth = 1 en entrée).
fn find_matching_close(html: &str, tag_name: &str) -> Option<usize> {
    let open = format!("<{tag_name}");
    let close = format!("</{tag_name}>");
    let mut depth = 1usize;
    let mut pos = 0;

    while pos < html.len() {
        let rest = &html[pos..];
        if rest.starts_with(&close) {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
            pos += close.len();
        } else if rest.starts_with(&open) {
            // Vérifie que c'est bien une balise ouvrante (suivie de > ou d'un blanc)
            let after_open = &rest[open.len()..];
            if after_open.starts_with(|c: char| !c.is_alphanumeric() && c != '-') {
                depth += 1;
            }
            pos += 1;
        } else {
            pos += 1;
        }
    }
    None
}

/// Analyse récursivement le sous-ensemble HTML inline produit par `execCommand`
/// et construit les nœuds `Plain` / `Span` correspondants sous `parent`.
fn parse_inline_html(html: &str, body: &mut Body, parent: BodyNodeId) -> anyhow::Result<()> {
    let mut remaining = html;

    while !remaining.is_empty() {
        if remaining.starts_with('<') {
            // Balise fermante orpheline → fin de récursion
            if remaining.starts_with("</") {
                break;
            }

            let tag_end = remaining
                .find('>')
                .map(|i| i + 1)
                .unwrap_or(remaining.len());
            let raw_tag = &remaining[..tag_end];
            remaining = &remaining[tag_end..];

            // Nom de la balise (minuscules)
            let tag_name: String = raw_tag[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();

            let close_tag = format!("</{tag_name}>");
            let close_pos = find_matching_close(remaining, &tag_name);
            let inner = close_pos.map(|p| &remaining[..p]).unwrap_or("");
            remaining = close_pos
                .map(|p| &remaining[p + close_tag.len()..])
                .unwrap_or("");

            let span_spec = match tag_name.as_str() {
                "b" | "strong" => Some(content::Span {
                    bold: true,
                    ..Default::default()
                }),
                "i" | "em" => Some(content::Span {
                    italic: true,
                    ..Default::default()
                }),
                "u" => Some(content::Span {
                    underline: true,
                    ..Default::default()
                }),
                "s" | "del" | "strike" => Some(content::Span {
                    strikeout: true,
                    ..Default::default()
                }),
                _ => None,
            };

            if let Some(spec) = span_spec {
                let span_id = body.create_node(NodeSpec::Span(spec));
                body.append_child_unchecked(parent, span_id)?;
                parse_inline_html(inner, body, span_id)?;
                // Supprimer le Span vide (ex : <b></b>)
                if body.children_of(span_id).is_empty() {
                    body.remove_subtree(span_id)?;
                }
            } else {
                // Balise inconnue : récurser directement dans parent
                parse_inline_html(inner, body, parent)?;
            }
        } else {
            let text_end = remaining.find('<').unwrap_or(remaining.len());
            let text = decode_html_entities(&remaining[..text_end]);
            remaining = &remaining[text_end..];
            if !text.is_empty() {
                let plain = body.create_node(NodeSpec::Plain(text));
                body.append_child_unchecked(parent, plain)?;
            }
        }
    }
    Ok(())
}

/// Remplace les enfants inline (Plain / Span) de `node_id` par l'arbre
/// correspondant au HTML `html`.
fn save_rich_content(body: &mut Body, node_id: BodyNodeId, html: &str) -> anyhow::Result<()> {
    for child in body.children_of(node_id) {
        body.remove_subtree(child)?;
    }
    parse_inline_html(html, body, node_id)?;
    // Garantir au moins un Plain (invariant feuille)
    if body.children_of(node_id).is_empty() {
        let plain = body.create_node(NodeSpec::Plain(String::new()));
        body.append_child_unchecked(node_id, plain)?;
    }
    Ok(())
}

// ─── ContentSubtree ───────────────────────────────────────────────────────────

/// Dispatcher des nœuds de contenu d'un nœud (Paragraphe, Table, List…).
///
/// Filtre les enfants directs qui sont des nœuds de contenu et délègue
/// à [`EditParagraph`], [`EditList`] ou [`EditTable`].
#[component]
pub fn ContentSubtree(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    view! {
        <div class="content-subtree space-y-1">
            <For
                each=move || ctx.body.with(|b| {
                    b.children_of(node_id).into_iter()
                        .filter(|&id| matches!(
                            b.kind_of(id),
                            NodeKind::Paragraphe | NodeKind::Table | NodeKind::List
                        ))
                        .collect::<Vec<_>>()
                })
                key=|id| *id
                children=move |id| {
                    let kind = ctx.body.with_untracked(|b| b.kind_of(id));
                    match kind {
                        NodeKind::Paragraphe => view! { <EditParagraph node_id=id/> }.into_any(),
                        NodeKind::List       => view! { <EditList node_id=id/> }.into_any(),
                        NodeKind::Table      => view! { <EditTable node_id=id/> }.into_any(),
                        _ => view! { <span/> }.into_any(),
                    }
                }
            />
        </div>
    }
}

// ─── Paragraphe ───────────────────────────────────────────────────────────────

#[component]
fn EditParagraph(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    let html = Signal::derive(move || ctx.body.with(|b| build_inline_html(b, node_id)));

    let on_enter = Callback::new(move |()| {
        let new_id = ctx
            .body
            .try_update(|b| -> Option<BodyNodeId> {
                let new_para = b.create_node(NodeSpec::Paragraphe);
                let plain = b.create_node(NodeSpec::Plain(String::new()));
                b.append_child_unchecked(new_para, plain).ok()?;
                b.insert_sibling_after(node_id, new_para).ok()?;
                Some(new_para)
            })
            .flatten();
        if let Some(id) = new_id {
            ctx.request_focus(id, false);
        }
    });

    let on_backspace_start = Callback::new(move |()| {
        let prev = ctx
            .body
            .try_update(|b| -> Option<BodyNodeId> { b.merge_with_prev(node_id).ok() })
            .flatten();
        if let Some(id) = prev {
            ctx.request_focus(id, true);
        }
    });

    let on_delete_end = Callback::new(move |()| {
        ctx.body.update(|b| {
            if let Some(next) = b.next_sibling_of(node_id) {
                let _ = b.merge_into(node_id, next);
            }
        });
    });

    view! {
        <p class="my-1 text-sm">
            <RichEditableDiv
                html=html
                on_save=move |h| {
                    ctx.body.update(|b| { let _ = save_rich_content(b, node_id, &h); });
                }
                focus_node_id=node_id
                on_enter=on_enter
                on_backspace_start=on_backspace_start
                on_delete_end=on_delete_end
            />
        </p>
    }
}

// ─── Liste ────────────────────────────────────────────────────────────────────

pub(super) fn is_list_ordered(body: &impl BodyRead, node_id: BodyNodeId) -> bool {
    if let NodeSpec::List(list) = body.spec_of(node_id) {
        !matches!(
            list.marker,
            content::ListMarker::Disc | content::ListMarker::Circle | content::ListMarker::Square
        )
    } else {
        false
    }
}

#[component]
fn EditList(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    let is_ordered = Signal::derive(move || ctx.body.with(|b| is_list_ordered(b, node_id)));

    view! {
        <div class="my-1 text-sm">
            {move || if is_ordered.get() {
                view! {
                    <ol class="list-decimal list-outside pl-5 space-y-0.5">
                        <For
                            each=move || ctx.body.with(|b| b.children_of(node_id))
                            key=|id| *id
                            children=move |id| view! { <EditListItem node_id=id list_id=node_id/> }
                        />
                    </ol>
                }.into_any()
            } else {
                view! {
                    <ul class="list-disc list-outside pl-5 space-y-0.5">
                        <For
                            each=move || ctx.body.with(|b| b.children_of(node_id))
                            key=|id| *id
                            children=move |id| view! { <EditListItem node_id=id list_id=node_id/> }
                        />
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn EditListItem(node_id: BodyNodeId, list_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    let html = Signal::derive(move || ctx.body.with(|b| build_inline_html(b, node_id)));

    let on_enter = Callback::new(move |()| {
        let new_id = ctx
            .body
            .try_update(|b| -> Option<BodyNodeId> {
                let item = b.create_node(NodeSpec::ListItem(content::ListItem::default()));
                let plain = b.create_node(NodeSpec::Plain(String::new()));
                b.append_child_unchecked(item, plain).ok()?;
                b.insert_sibling_after(node_id, item).ok()?;
                Some(item)
            })
            .flatten();
        if let Some(id) = new_id {
            ctx.request_focus(id, false);
        }
    });

    let on_backspace_start = Callback::new(move |()| {
        let is_empty = ctx
            .body
            .with_untracked(|b| build_inline_html(b, node_id).is_empty());

        if is_empty {
            // Élément vide : supprimer et focaliser le précédent/suivant
            let focus_id = ctx.body.with_untracked(|b| {
                b.prev_sibling_of(node_id)
                    .or_else(|| b.next_sibling_of(node_id))
                    .or_else(|| b.prev_sibling_of(list_id))
            });
            ctx.body.update(|b| {
                let _ = b.remove_node(node_id);
            });
            if let Some(id) = focus_id {
                ctx.request_focus(id, true);
            }
        } else {
            // Élément non vide : fusionner ou convertir en paragraphe
            let result = ctx
                .body
                .try_update(|b| -> Option<BodyNodeId> {
                    if b.prev_sibling_of(node_id).is_some() {
                        // Il y a un élément précédent : fusionner
                        b.merge_with_prev(node_id).ok()
                    } else {
                        // Premier élément : convertir en Paragraphe avant la liste
                        let parent = b.parent_of(list_id)?;
                        let siblings = b.children_of(parent);
                        let list_pos = siblings.iter().position(|&id| id == list_id)?;
                        let new_para = b.create_node(NodeSpec::Paragraphe);
                        b.merge_into(new_para, node_id).ok()?;
                        if b.children_of(new_para).is_empty() {
                            let plain = b.create_node(NodeSpec::Plain(String::new()));
                            b.append_child_unchecked(new_para, plain).ok()?;
                        }
                        b.insert_child_at_unchecked(parent, list_pos, new_para)
                            .ok()?;
                        if b.children_of(list_id).is_empty() {
                            let _ = b.remove_node(list_id);
                        }
                        Some(new_para)
                    }
                })
                .flatten();
            if let Some(id) = result {
                ctx.request_focus(id, true);
            }
        }
    });

    view! {
        <li class="group/item">
            <div class="flex items-baseline gap-1">
                <div class="flex-1">
                    <RichEditableDiv
                        html=html
                        on_save=move |h| {
                            ctx.body.update(|b| { let _ = save_rich_content(b, node_id, &h); });
                        }
                        focus_node_id=node_id
                        on_enter=on_enter
                        on_backspace_start=on_backspace_start
                    />
                </div>
                <button
                    class="no-print opacity-0 group-hover/item:opacity-100 text-red-400 hover:text-red-600 text-xs"
                    on:click=move |_| {
                        ctx.body.update(|b| { let _ = b.remove_node(node_id); });
                    }
                >"×"</button>
            </div>
        </li>
    }
}

// ─── Tableau ──────────────────────────────────────────────────────────────────

#[component]
fn EditTable(node_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    view! {
        <div class="my-2 overflow-x-auto">
            <table class="border-collapse w-full text-sm">
                <tbody>
                    <For
                        each=move || ctx.body.with(|b| b.children_of(node_id))
                        key=|id| *id
                        children=move |id| view! { <EditTableRow node_id=id table_id=node_id/> }
                    />
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn EditTableRow(node_id: BodyNodeId, table_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    view! {
        <tr class="group/row border-b border-gray-200">
            <For
                each=move || ctx.body.with(|b| b.children_of(node_id))
                key=|id| *id
                children=move |id| view! { <EditTableCell node_id=id row_id=node_id table_id=table_id/> }
            />
            <td class="w-10 px-1 align-middle border border-gray-200 bg-gray-50">
                <div class="flex flex-col gap-0.5 items-center">
                    <button
                        title="Ajouter une cellule à droite"
                        class="no-print opacity-0 group-hover/row:opacity-100 text-teal-600 hover:text-teal-800 text-xs font-bold leading-none"
                        on:click=move |_| {
                            ctx.body.update(|b| { let _ = b.append_node(node_id, NodeSpec::TableCell); });
                        }
                    >"+"</button>
                    <button
                        title="Insérer une ligne après"
                        class="no-print opacity-0 group-hover/row:opacity-100 text-blue-500 hover:text-blue-700 text-xs leading-none"
                        on:click=move |_| {
                            let first_cell = ctx.body.try_update(|b| -> Option<BodyNodeId> {
                                let col_count = b.children_of(node_id).len().max(1);
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
                                let _ = b.insert_sibling_after(node_id, new_row);
                                first
                            }).flatten();
                            if let Some(id) = first_cell {
                                ctx.request_focus(id, false);
                            }
                        }
                    >"↓"</button>
                    <button
                        title="Supprimer la ligne"
                        class="no-print opacity-0 group-hover/row:opacity-100 text-red-400 hover:text-red-600 text-xs leading-none"
                        on:click=move |_| {
                            ctx.body.update(|b| { let _ = b.remove_node(node_id); });
                        }
                    >"×"</button>
                </div>
            </td>
        </tr>
    }
}

#[component]
fn EditTableCell(node_id: BodyNodeId, row_id: BodyNodeId, table_id: BodyNodeId) -> impl IntoView {
    let ctx = expect_editor_context();

    let para_id = ctx.body.with_untracked(|b| b.first_child_of(node_id));

    let html = Signal::derive(move || {
        ctx.body.with(|b| {
            para_id
                .map(|pid| build_inline_html(b, pid))
                .unwrap_or_default()
        })
    });

    view! {
        <td
            class="border border-gray-200 px-2 py-1 min-w-[5rem] align-top"
            on:keydown=move |ev| {
                if ev.key() == "Tab" {
                    ev.prevent_default();
                    let forward = !ev.shift_key();
                    navigate_table_cell(ctx, node_id, row_id, table_id, forward);
                }
            }
        >
            <RichEditableDiv
                html=html
                on_save=move |h| {
                    ctx.body.update(|b| {
                        if let Some(pid) = para_id {
                            let _ = save_rich_content(b, pid, &h);
                        }
                    });
                }
                focus_node_id=node_id
            />
        </td>
    }
}

/// Navigation Tab entre cellules d'un tableau. Avance ou recule dans la liste
/// plate de toutes les cellules (ligne par ligne). En fin de tableau, Tab
/// crée automatiquement une nouvelle ligne et focalise sa première cellule.
fn navigate_table_cell(
    ctx: super::context::EditorContext,
    node_id: BodyNodeId,
    row_id: BodyNodeId,
    table_id: BodyNodeId,
    forward: bool,
) {
    let next = ctx.body.with_untracked(|b| {
        let all_cells: Vec<BodyNodeId> = b
            .children_of(table_id)
            .into_iter()
            .flat_map(|r| b.children_of(r))
            .collect();
        let pos = all_cells.iter().position(|&id| id == node_id)?;
        if forward {
            Some(all_cells.get(pos + 1).copied())
        } else {
            Some(pos.checked_sub(1).map(|i| all_cells[i]))
        }
    });

    match next {
        Some(Some(target_id)) => {
            ctx.request_focus(target_id, false);
        }
        Some(None) if forward => {
            // Tab sur la dernière cellule → ajoute une ligne
            let first_new_cell = ctx
                .body
                .try_update(|b| -> Option<BodyNodeId> {
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
                        if i == 0 {
                            first = Some(cell);
                        }
                    }
                    let _ = b.append_child_unchecked(table_id, new_row);
                    first
                })
                .flatten();
            if let Some(id) = first_new_cell {
                ctx.request_focus(id, false);
            }
        }
        _ => {}
    }
}
