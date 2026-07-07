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
        value
            .as_ref()
            .char_indices()
            .nth(self.offset)
            .map(|(i, _)| i)
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

/// Position de `anchor`/`focus` dans `leafs` (ordre du document), si les
/// deux y figurent et dans cet ordre. Partagé par [`Selection::covered_leafs`]
/// et [`Selection::extract_text`].
fn leaf_index_range(
    leafs: &[BodyNodeId],
    anchor: BodyNodeId,
    focus: BodyNodeId,
) -> Option<(usize, usize)> {
    let start_idx = leafs.iter().position(|&id| id == anchor)?;
    let end_idx = leafs.iter().position(|&id| id == focus)?;
    (start_idx <= end_idx).then_some((start_idx, end_idx))
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
        Self {
            anchor: cursor,
            focus: cursor,
        }
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

    /// Feuilles `Plain` couvertes par la sélection, dans l'ordre du
    /// document (bornes incluses). Utilisé pour détecter qu'une section
    /// supprimée invalide un commentaire (voir
    /// [`crate::editor::context::EditorContext::remove_node_with_comments`])
    /// et pour surligner les zones commentées dans l'éditeur (voir
    /// `crate::editor::content`). Vide si `anchor`/`focus` ne sont pas dans
    /// l'ordre du document ou si l'un des deux nœuds est introuvable.
    pub fn covered_leafs<B: BodyRead + ?Sized>(&self, body: &B) -> Vec<BodyNodeId> {
        let leafs = body.leafs();
        let Some((start_idx, end_idx)) =
            leaf_index_range(&leafs, self.anchor.node_id, self.focus.node_id)
        else {
            return Vec::new();
        };
        leafs[start_idx..=end_idx].to_vec()
    }

    /// Extrait le texte recouvert par la sélection, en concaténant les
    /// portions de chaque feuille `Plain` traversée entre `anchor` et
    /// `focus` (bornes incluses). Utilisé pour figer l'extrait affiché dans
    /// la bulle d'un commentaire (voir [`crate::traits::review::Comment::excerpt`]).
    /// Renvoie une chaîne vide si `anchor`/`focus` ne sont pas dans l'ordre
    /// du document ou si l'un des deux nœuds est introuvable.
    pub fn extract_text<B: BodyRead + ?Sized>(&self, body: &B) -> String {
        let leafs = body.leafs();
        let Some((start_idx, end_idx)) =
            leaf_index_range(&leafs, self.anchor.node_id, self.focus.node_id)
        else {
            return String::new();
        };

        let mut out = String::new();
        for (offset, &leaf) in leafs[start_idx..=end_idx].iter().enumerate() {
            let idx = start_idx + offset;
            let text = body.text_of(leaf);
            if idx == start_idx && idx == end_idx {
                let (from, to) = (
                    self.anchor.offset.min(self.focus.offset),
                    self.anchor.offset.max(self.focus.offset),
                );
                out.extend(text.chars().skip(from).take(to.saturating_sub(from)));
            } else if idx == start_idx {
                out.extend(text.chars().skip(self.anchor.offset));
            } else if idx == end_idx {
                out.extend(text.chars().take(self.focus.offset));
            } else {
                out.push_str(&text);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;
    use crate::direct::DirectBody;
    use crate::kind::Article;
    use crate::traits::node::{BodyRead, BodyWrite};
    use crate::NodeSpec;

    fn paragraph_plain(body: &mut DirectBody) -> BodyNodeId {
        let article = body
            .append_node(body.root(), NodeSpec::Article(Article::default()))
            .unwrap();
        let article_body = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
            .unwrap();
        let paragraphe = body
            .children_of(article_body)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::Paragraphe)
            .unwrap();
        body.first_child_of(paragraphe).unwrap()
    }

    #[test]
    fn test_extract_text_within_single_leaf() {
        let mut body = DirectBody::new();
        let plain = paragraph_plain(&mut body);
        body.insert_text(plain, 0, "hello world");

        let selection = Selection {
            anchor: Cursor {
                node_id: plain,
                offset: 6,
            },
            focus: Cursor {
                node_id: plain,
                offset: 11,
            },
        };
        assert_eq!(selection.extract_text(&body), "world");
    }

    #[test]
    fn test_extract_text_across_leafs() {
        let mut body = DirectBody::new();
        let plain = paragraph_plain(&mut body);
        body.insert_text(plain, 0, "hello world");
        let tail = body.split_node(plain, 5).unwrap(); // "hello" | " world"

        let selection = Selection {
            anchor: Cursor {
                node_id: plain,
                offset: 3,
            },
            focus: Cursor {
                node_id: tail,
                offset: 3,
            },
        };
        assert_eq!(selection.extract_text(&body), "lo wo");
    }

    #[test]
    fn test_extract_text_unknown_node_is_empty() {
        let mut body = DirectBody::new();
        let plain = paragraph_plain(&mut body);
        body.insert_text(plain, 0, "hello");

        let selection = Selection {
            anchor: Cursor {
                node_id: plain,
                offset: 0,
            },
            focus: Cursor {
                node_id: BodyNodeId::new(),
                offset: 0,
            },
        };
        assert_eq!(selection.extract_text(&body), "");
    }
}
