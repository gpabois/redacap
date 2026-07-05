use strum_macros::AsRefStr;

use crate::{ContentHandle, ContentId, ContentRead, Cursor};

#[derive(Default, Debug, Clone, Copy)]
pub struct EditorSelection {
    pub(super) state: EditorSelectionState,
    pub(super) anchor: Option<Cursor>,
    pub(super) focus: Option<Cursor>,
}

impl EditorSelection {
    pub fn correct(&mut self, body: &ContentHandle) {
        if let Some(focus) = self.focus
            && let Some(anchor) = self.anchor
            && focus.partial_cmp(&anchor, body) == Some(std::cmp::Ordering::Less)
        {
            std::mem::swap(&mut self.anchor, &mut self.focus);
        }
    }

    pub fn is_plain_selected(&self, content_id: ContentId, body: &ContentHandle) -> Selection {
        let Some(anchor) = self.anchor else {
            return Selection::Nothing;
        };
        let Some(focus) = self.focus else {
            return Selection::Nothing;
        };

        if body.leaf_order_of(content_id, anchor.content_id) == Some(std::cmp::Ordering::Less) {
            return Selection::Nothing;
        }

        if body.leaf_order_of(content_id, focus.content_id) == Some(std::cmp::Ordering::Greater) {
            return Selection::Nothing;
        }

        if content_id == anchor.content_id && focus.content_id == content_id {
            return Selection::Span(anchor.offset, focus.offset);
        }

        if content_id == anchor.content_id {
            return Selection::Span(anchor.offset, body.len_of(content_id));
        }

        if content_id == focus.content_id {
            return Selection::Span(0, focus.offset);
        }

        Selection::Span(0, body.len_of(content_id))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, AsRefStr)]
pub enum EditorSelectionState {
    #[default]
    Idle,
    Dragging,
}

pub enum Selection {
    Span(usize, usize),
    Nothing,
}
