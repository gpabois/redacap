use leptos::ev::KeyboardEvent;
use web_sys::MouseEvent;

use crate::Cursor;

pub enum EditorEvent {
    MouseDown(MouseEvent),
    MouseClick(MouseEvent),
    MouseUp(MouseEvent),
    MouseEnter(MouseEvent),
    MouseMove(MouseEvent),
    KeyDown(KeyboardEvent),
    AnchorSet(Cursor),
    FocusSet(Cursor),
    CursorMoved(Cursor),
    Focus,
    Blur,
    StringWritten(usize),
    CharAdded,
    CharRemoved,
}
