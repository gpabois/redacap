use leptos::context::use_context;

use crate::editor::{EditorState, state::ContentEditorState};

pub fn use_editor_state() -> EditorState {
    use_context().unwrap()
}

pub fn use_content_editor_state() -> ContentEditorState {
    use_context().unwrap()
}