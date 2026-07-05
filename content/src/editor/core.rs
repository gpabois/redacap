use std::collections::VecDeque;

use leptos::prelude::*;
use web_sys::MouseEvent;

use crate::{ContentHandle, ContentId, ContentRead, ContentWrite, Cursor};

use super::context::EditorCursor;
use super::event::EditorEvent;
use super::merge::merge_leaves;
use super::polyfill::raycast_text_node;
use super::selection::{EditorSelection, EditorSelectionState};

#[derive(Clone, Copy)]
pub struct Editor {
    pub(super) body: RwSignal<ContentHandle>,
    pub(super) cursor: RwSignal<EditorCursor>,
    pub(super) selection: RwSignal<EditorSelection>,
    pub(super) ev_queue: StoredValue<VecDeque<EditorEvent>>,
}

impl Editor {
    pub fn send_event(self, ev: EditorEvent) {
        self.ev_queue.update_value(|ev_queue| {
            ev_queue.push_back(ev);
            self.schedule_event_loop();
        });
    }

    pub fn schedule_event_loop(self) {
        request_animation_frame(move || {
            self.ev_queue.update_value(|ev_queue| {
                while let Some(event) = ev_queue.pop_front() {
                    self.process_event(event);
                }
            });
        });
    }

    pub fn process_event(self, ev: EditorEvent) {
        use EditorEvent::*;

        match ev {
            KeyDown(e) if e.key() == "ArrowLeft" => self.on_arrow_left(),
            KeyDown(e) if e.key() == "ArrowRight" => self.on_arrow_right(),
            KeyDown(e) if e.key() == "ArrowUp" => {}
            KeyDown(e) if e.key() == "ArrowDown" => {}
            KeyDown(e) if e.key() == "Backspace" => self.on_backspace(),
            KeyDown(e) if e.key() == "Delete" => self.on_delete(),
            KeyDown(e) if e.key() == "Enter" => self.on_enter(&e),
            MouseDown(ev) => self.on_mouse_down(ev),
            MouseUp(ev) => self.on_mouse_up(ev),
            MouseMove(ev) => self.on_mouse_move(ev),
            MouseClick(ev) => self.on_mouse_click(ev),
            Focus => self.on_focus(),
            Blur => self.on_blur(),
            StringWritten(len) => self.on_string_written(len),
            CharAdded => self.on_char_added(),
            CharRemoved => self.on_char_removed(),
            FocusSet(cursor) => self.on_cursor_moved(cursor),
            CursorMoved(cursor) => self.on_cursor_moved(cursor),
            _ => {}
        }
    }

    fn on_arrow_left(self) {
        self.move_cursor_to_left();
    }

    fn on_arrow_right(self) {
        self.move_cursor_to_right();
    }

    /// Supprime le caractère précédant le curseur ; en début de noeud,
    /// fusionne avec la feuille précédente (voir [`merge_leaves`]).
    fn on_backspace(self) {
        self.body.update(|body| {
            let caret = self.cursor.get_untracked().caret;

            if caret.offset > 0 {
                body.remove_text(caret.content_id, caret.offset - 1, 1);
                self.send_event(EditorEvent::CharRemoved);
            } else if let Some(prev) = body.prev_leaf_of(caret.content_id) {
                let offset = body.len_of(prev);
                merge_leaves(body, prev, caret.content_id);
                self.send_event(EditorEvent::CursorMoved(Cursor {
                    content_id: prev,
                    offset,
                }));
            }
        });
    }

    /// Supprime le caractère suivant le curseur ; en fin de noeud, fusionne
    /// avec la feuille suivante (voir [`merge_leaves`]).
    fn on_delete(self) {
        self.body.update(|body| {
            let caret = self.cursor.get_untracked().caret;

            if caret.offset < body.len_of(caret.content_id) {
                body.remove_text(caret.content_id, caret.offset, 1);
            } else if let Some(next) = body.next_leaf_of(caret.content_id) {
                merge_leaves(body, caret.content_id, next);
            }
        });
    }

    /// Divise la feuille courante et son paragraphe au niveau du curseur.
    fn on_enter(self, e: &leptos::ev::KeyboardEvent) {
        e.prevent_default();

        self.body.update(|body| {
            let caret = self.cursor.get_untracked().caret;

            if let Ok(new_leaf) = body.split_node(caret.content_id, caret.offset)
                && let Some(paragraph) = body.parent_of(caret.content_id)
                && let Some(index) = body
                    .children_of(paragraph)
                    .iter()
                    .position(|&c| c == new_leaf)
                && body.split_node(paragraph, index).is_ok()
            {
                self.send_event(EditorEvent::CursorMoved(Cursor {
                    content_id: new_leaf,
                    offset: 0,
                }));
            }
        });
    }

    fn on_mouse_down(self, ev: MouseEvent) {
        self.selection.update(|sel| {
            use EditorSelectionState::{Dragging, Idle};
            if let Idle = sel.state {
                let x = ev.x() as f32;
                let y = ev.y() as f32;

                if let Some(cursor) = self.search_cursor_at(x, y) {
                    sel.state = Dragging;
                    sel.anchor = Some(cursor);
                    sel.focus = None;
                    self.send_event(EditorEvent::AnchorSet(cursor));
                    ev.prevent_default();
                }
            }
        });
    }

    fn on_mouse_up(self, _ev: MouseEvent) {
        self.selection.update(|sel| {
            use EditorSelectionState::{Dragging, Idle};
            if let Dragging = sel.state {
                sel.state = Idle;
            }
        });
    }

    fn on_mouse_move(self, ev: MouseEvent) {
        self.cursor.update(|cursor| {
            let x = ev.x() as f32;
            let y = ev.y() as f32;

            if let Some(mouse) = self.search_cursor_at(x, y) {
                cursor.mouse = mouse;
            }
        });
        self.selection.update(|sel| {
            use EditorSelectionState::Dragging;
            if let Dragging = sel.state {
                let x = ev.x() as f32;
                let y = ev.y() as f32;

                if let Some(cursor) = self.search_cursor_at(x, y) {
                    sel.focus = Some(cursor);
                    sel.correct(&self.body.read_untracked());
                    self.send_event(EditorEvent::FocusSet(sel.focus.unwrap()));
                }
            }
        });
    }

    fn on_mouse_click(self, ev: MouseEvent) {
        let x = ev.x() as f32;
        let y = ev.y() as f32;

        if let Some(cursor) = self.search_cursor_at(x, y) {
            self.move_cursor(cursor);
        }
    }

    fn on_focus(self) {
        self.cursor.update(|cursor| cursor.display = true);
    }

    fn on_blur(self) {
        self.cursor.update(|cursor| cursor.display = false);
    }

    fn on_string_written(self, len: usize) {
        self.cursor.update(move |cursor| {
            (0..len).for_each(|_| cursor.right(&*self.body.read_untracked()))
        });
    }

    fn on_char_added(self) {
        self.move_cursor_to_right();
    }

    fn on_char_removed(self) {
        self.move_cursor_to_left();
    }

    fn on_cursor_moved(self, cursor: Cursor) {
        self.move_cursor(cursor);
    }

    pub fn write_str(self, str: &str) {
        self.body.update(|body| {
            let cursor = self.cursor.get();
            body.insert_text(cursor.content_id, cursor.offset, str);
            self.send_event(EditorEvent::StringWritten(str.chars().count()));
        });
    }

    pub fn search_cursor_at(self, x: f32, y: f32) -> Option<Cursor> {
        let document = document();
        let (node, offset) = raycast_text_node(&document, x, y)?;
        let mut node = node.parent_element()?;

        loop {
            if let Some(attr) = node.get_attribute("data-content-id") {
                let offset: usize = node
                    .get_attribute("data-content-offset")
                    .and_then(|off| off.parse().ok())
                    .unwrap_or(offset)
                    + offset;

                let content_id: ContentId = attr.parse().ok()?;
                return Some(Cursor { offset, content_id });
            } else if let Some(parent) = node.parent_element() {
                node = parent;
            } else {
                break;
            }
        }

        None
    }

    pub fn move_cursor(self, value: Cursor) {
        self.cursor.update(|cursor| {
            cursor.content_id = value.content_id;
            cursor.offset = value.offset;
        });
    }

    pub fn move_cursor_to_left(self) {
        let body = self.body.read_untracked();
        self.cursor.update(move |cursor| cursor.left(&*body));
    }

    pub fn move_cursor_to_right(self) {
        let body = self.body.read_untracked();
        self.cursor.update(move |cursor| cursor.right(&*body));
    }
}
