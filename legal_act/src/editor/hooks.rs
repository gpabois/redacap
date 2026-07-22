use leptos::context::use_context;

use crate::editor::EditorState;

pub fn use_editor_state() -> EditorState {
    use_context().unwrap()
}