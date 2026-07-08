use anyhow::bail;
use yrs::{Any, Array, ArrayPrelim, ArrayRef, Doc, Map, MapPrelim, MapRef, Out, ReadTxn, Transact};

use crate::BodyNodeId;
use crate::cursor::{Cursor, Selection};
use crate::traits::review::{Comment, CommentId, ReviewRead, ReviewWrite, WorkNote};

/// Backend "mode Yrs" pour les commentaires et notes de travail d'un
/// projet : porté par un [`yrs::Doc`] et synchronisable entre pairs via
/// CRDT (voir [`crate::YrsBody`] pour le pendant côté corps de l'acte).
///
/// Les commentaires et notes sont stockés dans deux tableaux (`comments`,
/// `work_notes`) de la map racine `review`, chaque élément étant une
/// [`yrs::MapRef`] portant les champs du commentaire.
pub struct YrsReview {
    doc: Doc,
    #[allow(dead_code)]
    review: MapRef,
    comments: ArrayRef,
    work_notes: ArrayRef,
}

impl YrsReview {
    pub fn new() -> Self {
        let doc = Doc::new();
        let review = doc.get_or_insert_map("review");
        Self::init(doc, review)
    }

    pub fn init(doc: Doc, review: MapRef) -> Self {
        let mut txn = doc.transact_mut();
        let comments = review.insert(&mut txn, "comments", ArrayPrelim::default());
        let work_notes = review.insert(&mut txn, "work_notes", ArrayPrelim::default());
        drop(txn);
        Self {
            doc,
            review,
            comments,
            work_notes,
        }
    }

    pub fn open(doc: Doc, review: MapRef) -> anyhow::Result<Self> {
        let txn = doc.transact();
        let Some(Out::YArray(comments)) = review.get(&txn, "comments") else {
            bail!("champ 'comments' manquant ou invalide dans le nœud review yrs");
        };
        let Some(Out::YArray(work_notes)) = review.get(&txn, "work_notes") else {
            bail!("champ 'work_notes' manquant ou invalide dans le nœud review yrs");
        };
        drop(txn);
        Ok(Self {
            doc,
            review,
            comments,
            work_notes,
        })
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }
}

impl Default for YrsReview {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn find_index(array: &ArrayRef, txn: &impl ReadTxn, id: &str) -> Option<u32> {
    array.iter(txn).enumerate().find_map(|(i, out)| match out {
        Out::YMap(m) => match m.get(txn, "id") {
            Some(Out::Any(Any::String(s))) => (s.as_ref() == id).then_some(i as u32),
            _ => None,
        },
        _ => None,
    })
}

fn get_string(map: &MapRef, txn: &impl ReadTxn, key: &str) -> Option<String> {
    match map.get(txn, key) {
        Some(Out::Any(Any::String(s))) => Some(s.to_string()),
        _ => None,
    }
}

fn get_bool(map: &MapRef, txn: &impl ReadTxn, key: &str) -> bool {
    matches!(map.get(txn, key), Some(Out::Any(Any::Bool(true))))
}

fn get_u32(map: &MapRef, txn: &impl ReadTxn, key: &str) -> Option<u32> {
    match map.get(txn, key) {
        Some(Out::Any(Any::Number(n))) => Some(n as u32),
        _ => None,
    }
}

fn read_selection(map: &MapRef, txn: &impl ReadTxn) -> Option<Selection> {
    let Some(Out::YMap(sel)) = map.get(txn, "selection") else {
        return None;
    };
    let anchor_node: BodyNodeId = get_string(&sel, txn, "anchor_node")?.parse().ok()?;
    let anchor_offset = get_u32(&sel, txn, "anchor_offset")? as usize;
    let focus_node: BodyNodeId = get_string(&sel, txn, "focus_node")?.parse().ok()?;
    let focus_offset = get_u32(&sel, txn, "focus_offset")? as usize;
    Some(Selection {
        anchor: Cursor {
            node_id: anchor_node,
            offset: anchor_offset,
        },
        focus: Cursor {
            node_id: focus_node,
            offset: focus_offset,
        },
    })
}

fn selection_prelim(selection: &Selection) -> MapPrelim {
    MapPrelim::from_iter([
        (
            "anchor_node",
            yrs::In::from(selection.anchor.node_id.to_string()),
        ),
        (
            "anchor_offset",
            yrs::In::from(selection.anchor.offset as f64),
        ),
        (
            "focus_node",
            yrs::In::from(selection.focus.node_id.to_string()),
        ),
        ("focus_offset", yrs::In::from(selection.focus.offset as f64)),
    ])
}

fn read_comment(map: &MapRef, txn: &impl ReadTxn) -> anyhow::Result<Comment> {
    let id: CommentId = get_string(map, txn, "id")
        .ok_or_else(|| anyhow::anyhow!("champ 'id' manquant"))?
        .parse()?;
    Ok(Comment {
        id,
        author: get_string(map, txn, "author").unwrap_or_default(),
        text: get_string(map, txn, "text").unwrap_or_default(),
        selection: read_selection(map, txn),
        excerpt: get_string(map, txn, "excerpt"),
        parent: get_string(map, txn, "parent").and_then(|s| s.parse().ok()),
        resolved: get_bool(map, txn, "resolved"),
    })
}

fn comment_prelim(comment: &Comment) -> MapPrelim {
    let mut fields: Vec<(&str, yrs::In)> = vec![
        ("id", yrs::In::from(comment.id.to_string())),
        ("author", yrs::In::from(comment.author.clone())),
        ("text", yrs::In::from(comment.text.clone())),
        ("resolved", yrs::In::from(comment.resolved)),
    ];
    if let Some(parent) = comment.parent {
        fields.push(("parent", yrs::In::from(parent.to_string())));
    }
    if let Some(excerpt) = &comment.excerpt {
        fields.push(("excerpt", yrs::In::from(excerpt.clone())));
    }
    if let Some(selection) = &comment.selection {
        fields.push(("selection", yrs::In::from(selection_prelim(selection))));
    }
    MapPrelim::from_iter(fields)
}

fn read_note(map: &MapRef, txn: &impl ReadTxn) -> anyhow::Result<WorkNote> {
    let id: CommentId = get_string(map, txn, "id")
        .ok_or_else(|| anyhow::anyhow!("champ 'id' manquant"))?
        .parse()?;
    Ok(WorkNote {
        id,
        author: get_string(map, txn, "author").unwrap_or_default(),
        text: get_string(map, txn, "text").unwrap_or_default(),
        private: get_bool(map, txn, "private"),
    })
}

fn note_prelim(note: &WorkNote) -> MapPrelim {
    MapPrelim::from_iter([
        ("id", yrs::In::from(note.id.to_string())),
        ("author", yrs::In::from(note.author.clone())),
        ("text", yrs::In::from(note.text.clone())),
        ("private", yrs::In::from(note.private)),
    ])
}

// ── Traits ───────────────────────────────────────────────────────────────────

impl ReviewRead for YrsReview {
    fn comments(&self) -> Vec<Comment> {
        let txn = self.doc.transact();
        self.comments
            .iter(&txn)
            .filter_map(|out| match out {
                Out::YMap(m) => read_comment(&m, &txn).ok(),
                _ => None,
            })
            .collect()
    }

    fn work_notes(&self) -> Vec<WorkNote> {
        let txn = self.doc.transact();
        self.work_notes
            .iter(&txn)
            .filter_map(|out| match out {
                Out::YMap(m) => read_note(&m, &txn).ok(),
                _ => None,
            })
            .collect()
    }
}

impl ReviewWrite for YrsReview {
    fn add_comment(&mut self, comment: Comment) -> CommentId {
        let id = comment.id;
        let mut txn = self.doc.transact_mut();
        let len = self.comments.len(&txn);
        self.comments
            .insert(&mut txn, len, comment_prelim(&comment));
        id
    }

    fn resolve_comment(&mut self, id: CommentId) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let idx = find_index(&self.comments, &txn, &id.to_string())
            .ok_or_else(|| anyhow::anyhow!("commentaire introuvable : {id}"))?;
        let Some(Out::YMap(m)) = self.comments.get(&txn, idx) else {
            bail!("commentaire introuvable : {id}");
        };
        m.insert(&mut txn, "resolved", true);
        Ok(())
    }

    fn delete_comment(&mut self, id: CommentId) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let idx = find_index(&self.comments, &txn, &id.to_string())
            .ok_or_else(|| anyhow::anyhow!("commentaire introuvable : {id}"))?;
        self.comments.remove(&mut txn, idx);
        Ok(())
    }

    fn add_work_note(&mut self, note: WorkNote) -> CommentId {
        let id = note.id;
        let mut txn = self.doc.transact_mut();
        let len = self.work_notes.len(&txn);
        self.work_notes.insert(&mut txn, len, note_prelim(&note));
        id
    }

    fn delete_work_note(&mut self, id: CommentId) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let idx = find_index(&self.work_notes, &txn, &id.to_string())
            .ok_or_else(|| anyhow::anyhow!("note de travail introuvable : {id}"))?;
        self.work_notes.remove(&mut txn, idx);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BodyNodeId;

    fn selection() -> Selection {
        Selection {
            anchor: Cursor {
                node_id: BodyNodeId::new(),
                offset: 0,
            },
            focus: Cursor {
                node_id: BodyNodeId::new(),
                offset: 3,
            },
        }
    }

    #[test]
    fn test_add_resolve_delete_roundtrip() {
        let mut review = YrsReview::new();
        let id = review
            .add_comment(Comment::new("alice", "à revoir").with_selection(selection(), "extrait"));
        let comment = review.comment(id).unwrap();
        assert_eq!(comment.excerpt.as_deref(), Some("extrait"));
        assert!(!comment.resolved);

        review.resolve_comment(id).unwrap();
        assert!(review.comment(id).unwrap().resolved);

        review.delete_comment(id).unwrap();
        assert!(review.comment(id).is_none());
    }

    #[test]
    fn test_reply_thread_roundtrip() {
        let mut review = YrsReview::new();
        let root = review.add_comment(Comment::new("alice", "root"));
        let reply = review.add_comment(Comment::new("bob", "reply").reply_to(root));
        assert_eq!(review.replies_to(root).len(), 1);
        review.delete_comment_thread(root).unwrap();
        assert!(review.comment(root).is_none());
        assert!(review.comment(reply).is_none());
    }

    #[test]
    fn test_syncs_to_remote_doc() {
        use yrs::updates::decoder::Decode;

        let mut writer = YrsReview::new();
        writer.add_comment(Comment::new("alice", "texte partagé"));

        let update = writer
            .doc()
            .transact()
            .encode_diff_v1(&yrs::StateVector::default());
        let remote_doc = Doc::new();
        remote_doc
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&update).unwrap())
            .unwrap();

        let remote_review = remote_doc.get_or_insert_map("review");
        let reader = YrsReview::open(remote_doc, remote_review).unwrap();
        let comments = reader.comments();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "texte partagé");
    }
}
