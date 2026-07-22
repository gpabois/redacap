use crate::cursor::Cursor;

/// Événements traités par l'éditeur d'acte légal.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    KeyDown(leptos::ev::KeyboardEvent),
    MouseDown(web_sys::MouseEvent),
    MouseUp(web_sys::MouseEvent),
    MouseMove(web_sys::MouseEvent),
    MouseClick(web_sys::MouseEvent),
    Focus,
    Blur,
    StringWritten(usize),
    CharAdded,
    CharRemoved,
    CursorMoved(Cursor),
    FocusSet(Cursor),
    AnchorSet(Cursor),
}
