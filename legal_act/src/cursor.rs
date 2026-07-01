use std::cmp::Ordering;

use crate::{BodyNodeId, traits::node::BodyRead};

/// Position dans le corps d'un acte légal : un nœud terminal (`Plain`)
/// et un décalage exprimé en nombre de caractères depuis le début du texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub node_id: BodyNodeId,
    pub offset: usize,
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.node_id, self.offset)
    }
}

impl Cursor {
    /// Convertit l'offset en index d'octet dans `value`.
    pub fn into_byte_offset<S: AsRef<str>>(self, value: S) -> Option<usize> {
        value.as_ref().char_indices().nth(self.offset).map(|(i, _)| i)
    }

    /// Découpe `value` au point du curseur : (avant, après).
    pub fn split_clone<S: AsRef<str>>(self, value: S) -> (String, String) {
        let value = value.as_ref();
        match self.into_byte_offset(value) {
            None => (value.to_owned(), String::new()),
            Some(i) => (value[..i].to_owned(), value[i..].to_owned()),
        }
    }

    /// Vrai si le curseur pointe sur `node_id`.
    pub fn is_within(&self, node_id: BodyNodeId) -> bool {
        self.node_id == node_id
    }

    pub fn partial_cmp<B: BodyRead + ?Sized>(&self, rhs: &Cursor, body: &B) -> Option<Ordering> {
        if self.node_id == rhs.node_id {
            return self.offset.partial_cmp(&rhs.offset);
        }
        body.leaf_order_of(self.node_id, rhs.node_id)
    }

    /// Déplace le curseur d'un caractère vers la gauche, en sautant sur
    /// la feuille précédente si en début de nœud.
    pub fn left<B: BodyRead + ?Sized>(&mut self, body: &B) {
        if self.offset == 0 {
            if let Some(prev) = body.prev_leaf_of(self.node_id) {
                self.node_id = prev;
                self.offset = body.len_of(prev);
            }
        } else {
            self.offset -= 1;
        }
    }

    /// Déplace le curseur d'un caractère vers la droite, en sautant sur
    /// la feuille suivante si en fin de nœud.
    pub fn right<B: BodyRead + ?Sized>(&mut self, body: &B) {
        let len = body.len_of(self.node_id);
        if self.offset >= len {
            if let Some(next) = body.next_leaf_of(self.node_id) {
                self.node_id = next;
                self.offset = 0;
            }
        } else {
            self.offset += 1;
        }
    }
}

/// Sélection traversante dans le corps de l'acte : un ancre (début) et
/// un focus (fin), avec `anchor ≤ focus` dans l'ordre du document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Cursor,
    pub focus: Cursor,
}

impl Selection {
    pub fn collapsed(cursor: Cursor) -> Self {
        Self { anchor: cursor, focus: cursor }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    /// Corrige l'ordre pour que `anchor ≤ focus`.
    pub fn correct<B: BodyRead + ?Sized>(&mut self, body: &B) {
        if let Some(Ordering::Greater) = self.anchor.partial_cmp(&self.focus, body) {
            std::mem::swap(&mut self.anchor, &mut self.focus);
        }
    }

    pub fn contains<B: BodyRead + ?Sized>(&self, cursor: &Cursor, body: &B) -> bool {
        self.anchor.partial_cmp(cursor, body) != Some(Ordering::Greater)
            && cursor.partial_cmp(&self.focus, body) != Some(Ordering::Greater)
    }
}
