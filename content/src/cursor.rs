use std::cmp::Ordering;

use crate::{ContentId, ContentRead};

/// Position dans le document : un noeud terminal (`Plain`) et un décalage
/// exprimé en nombre de caractères (et non en octets) depuis le début de son
/// texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub content_id: ContentId,
    pub offset: usize,
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.content_id, self.offset)
    }
}

impl Cursor {
    pub fn into_byte_offset<S: AsRef<str>>(self, value: S) -> Option<usize> {
        value.as_ref().char_indices().nth(self.offset).map(|(i, _)| i)
    }

    pub fn split_clone<S: AsRef<str>>(self, value: S) -> (String, String) {
        let value = value.as_ref();

        let Some(index) = self.into_byte_offset(value) else {
            return (value.to_owned(), String::default());
        };

        if value.len() <= self.offset {
            return (value.to_owned(), String::default());
        }

        let (lhs, rhs) = value.split_at(index);
        (lhs.to_owned(), rhs.to_owned())
    }

    pub fn is_content_within(&self, content_id: ContentId) -> bool {
        self.content_id == content_id
    }

    pub fn partial_cmp<B: ContentRead + ?Sized>(&self, rhs: &Cursor, body: &B) -> Option<Ordering> {
        if self.content_id == rhs.content_id {
            return self.offset.partial_cmp(&rhs.offset);
        }

        body.leaf_order_of(self.content_id, rhs.content_id)
    }

    pub fn left<B: ContentRead + ?Sized>(&mut self, body: &B) {
        if self.offset == 0 {
            if let Some(content_id) = body.prev_leaf_of(self.content_id) {
                self.content_id = content_id;
                self.offset = body.len_of(content_id);
            }
        } else {
            self.offset -= 1;
        }
    }

    pub fn right<B: ContentRead + ?Sized>(&mut self, body: &B) {
        let len = body.len_of(self.content_id);
        if self.offset >= len {
            if let Some(content_id) = body.next_leaf_of(self.content_id) {
                self.content_id = content_id;
                self.offset = 0;
            }
        } else {
            self.offset += 1;
        }
    }
}
