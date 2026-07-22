use leptos::{prelude::*, IntoView, context::provide_context, view};
use crate::{data::Paragraphe, editor::hooks::use_editor_state, id::NodeId, model::LegalActProject, str_ext::StrExt};

use super::EditorState;


#[component]
pub fn Editor(act: LegalActProject) {
    let state = EditorState::new(act);
    provide_context(state);



}

#[component]
pub fn Visas() -> impl IntoView {
    let state = use_editor_state();
    view! {
        <ul data-node-id="visas">
            <For
                each=move || state.visas().children()
                key=|id| id.clone()
                children=|id| view! { <Visa id={id}/> }
            />
        </ul>
    }
}

#[component]
pub fn Visa(id: NodeId) -> impl IntoView {
    let state = use_editor_state();
    let opts = EditOptions {
        plain: true,
        span: true,
        ..Default::default()
    };

    view! {
        <li>VU <ContentEdit root={id} options={opts} /></li>
    }
}

#[component]
pub fn ContentEdit(root: NodeId, options: EditOptions) -> impl IntoView {
    let state = use_editor_state();

    let textarea_ref = NodeRef::new();
    view!{
        <div data-node-id={root.clone().to_string()}>
            <textarea class="h-0 w-0" node_ref={textarea_ref}></textarea>
            <RenderChildren parent={root.clone()}/>
        </div>
    }
}

#[component]
pub fn RenderChildren(parent: NodeId) -> impl IntoView {
    let state = use_editor_state();

    view! {
        <For
            each=move || state.try_node(&parent).unwrap().children()
            key=|id| id.clone()
            children=|id| view! { <RenderContent id={id}/> }
        />
    }
}

#[component]
pub fn Cursor() -> impl IntoView {
    view! {<span>|</span>}
}

#[component]
pub fn Selected(children: Children) -> impl IntoView {
    view!{<span>{children()}</span>}
}

#[component]
pub fn RenderContent(id: NodeId) -> AnyView {
    let state = use_editor_state();
    let node = state.try_node(&id).unwrap();
    
    match node.kind() {
        crate::data::NodeKind::Paragraphe => view! {
            <p>
                <RenderChildren parent={id} />
            </p>
        }.into_any(),
        crate::data::NodeKind::Plain => {
            let text = node.text();
            let char_len = text.chars().count();

            let cursor = state.cursor.get().filter(|c| c.within(&id)).map(|c| c.pos);

            let selected = state.selection.get().and_then(|span| {
                match (span.start.id == id, span.end.id == id) {
                    (true, true) => Some((span.start.pos, span.end.pos)),
                    (true, false) => Some((span.start.pos, char_len)),
                    (false, true) => Some((0, span.end.pos)),
                    (false, false) => span.covers(state.act(), &id).then_some((0, char_len)),
                }
            });

            plain_view(&text, cursor, selected)
        },
        crate::data::NodeKind::Span => todo!(),
        crate::data::NodeKind::Table => todo!(),
        crate::data::NodeKind::TableRow => todo!(),
        crate::data::NodeKind::TableCell => todo!(),
        crate::data::NodeKind::List => todo!(),
        crate::data::NodeKind::ListItem => todo!(),
        _ => view! {}.into_any()
    }
}

/// Rend le texte d'un `Plain` en isolant la portion sélectionnée dans
/// `<Selected>` et en insérant `<Cursor/>` à la position du curseur.
///
/// Les positions de coupe (curseur, début et fin de la sélection locale à ce
/// nœud) sont résolues via [`StrExt::split_at_positions`].
fn plain_view(text: &str, cursor: Option<usize>, selected: Option<(usize, usize)>) -> AnyView {
    let char_len = text.chars().count();

    let mut positions: Vec<usize> = [cursor, selected.map(|(start, _)| start), selected.map(|(_, end)| end)]
        .into_iter()
        .flatten()
        .collect();
    positions.sort_unstable();
    positions.dedup();

    if positions.is_empty() {
        return text.to_string().into_any();
    }

    let parts = text.split_at_positions(&positions);

    let mut boundaries = Vec::with_capacity(positions.len() + 2);
    boundaries.push(0);
    boundaries.extend_from_slice(&positions);
    boundaries.push(char_len);

    let mut views = Vec::with_capacity(parts.len() * 2);

    for (i, part) in parts.into_iter().enumerate() {
        let seg_start = boundaries[i];
        let seg_end = boundaries[i + 1];

        if cursor == Some(seg_start) {
            views.push(view! { <Cursor/> }.into_any());
        }

        let is_selected = seg_start < seg_end
            && selected.is_some_and(|(start, end)| start <= seg_start && seg_end <= end);

        let owned = part.to_string();
        views.push(if is_selected {
            view! { <Selected>{owned}</Selected> }.into_any()
        } else {
            owned.into_any()
        });
    }

    if cursor == Some(char_len) {
        views.push(view! { <Cursor/> }.into_any());
    }

    views.into_any()
}

#[derive(Default)]
pub struct EditOptions {
    paragraph: bool,
    plain: bool,
    span: bool,
    list: bool,
    table: bool
}