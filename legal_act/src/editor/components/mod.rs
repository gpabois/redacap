use indextree::NodeId;
use leptos::{IntoView, attr::For, context::provide_context, view};

use crate::{editor::hooks::use_editor_state, model::LegalActProject};

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
                each=|| state.visas().children()
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

    view!{
        <div data-node-id={id}>
            <textarea></textarea>
        </div>
    }
}

#[derive(Default)]
pub struct EditOptions {
    paragraph: bool,
    plain: bool,
    span: bool,
    list: bool,
    table: bool
}