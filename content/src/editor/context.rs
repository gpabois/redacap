use std::ops::{Deref, DerefMut};

use leptos::prelude::*;
use shared::id::ID;

use crate::{ContentHandle, ContentId, ContentRead, Cursor};

use super::core::Editor;
use super::selection::EditorSelection;

#[derive(Debug, Clone, Copy)]
pub struct EditorCursor {
    pub(super) id: ID,
    pub(super) caret: Cursor,
    pub(super) mouse: Cursor,
    pub(super) display: bool,
}

impl DerefMut for EditorCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.caret
    }
}

impl Deref for EditorCursor {
    type Target = Cursor;

    fn deref(&self) -> &Self::Target {
        &self.caret
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CurrentContentId(pub(super) ContentId);

pub fn use_editor_content_body() -> RwSignal<ContentHandle> {
    use_context::<Editor>().unwrap().body
}

pub fn use_editor_cursor() -> RwSignal<EditorCursor> {
    use_context::<Editor>().unwrap().cursor
}

pub fn use_editor_selection() -> RwSignal<EditorSelection> {
    use_context::<Editor>().unwrap().selection
}

pub fn use_editor() -> Editor {
    use_context::<Editor>().unwrap()
}

pub fn use_current_content_id() -> ContentId {
    use_context::<CurrentContentId>().unwrap().0
}

pub fn use_current_text() -> impl Fn() -> String {
    let body = use_editor_content_body();
    let node_id = use_current_content_id();

    move || body.read().text_of(node_id)
}

pub fn use_current_children() -> impl Fn() -> Vec<ContentId> {
    let body = use_editor_content_body();
    let node_id = use_current_content_id();

    move || body.read().children_of(node_id)
}
