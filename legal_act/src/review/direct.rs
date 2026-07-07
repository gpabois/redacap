use anyhow::bail;

use crate::traits::review::{Comment, CommentId, ReviewRead, ReviewWrite, WorkNote};

/// Backend "mode direct" pour les commentaires et notes de travail d'un
/// projet : stockage en mémoire locale, sans CRDT (voir [`crate::DirectBody`]
/// pour le pendant côté corps de l'acte).
#[derive(Debug, Default)]
pub struct DirectReview {
    comments: Vec<Comment>,
    work_notes: Vec<WorkNote>,
}

impl DirectReview {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ReviewRead for DirectReview {
    fn comments(&self) -> Vec<Comment> {
        self.comments.clone()
    }

    fn work_notes(&self) -> Vec<WorkNote> {
        self.work_notes.clone()
    }
}

impl ReviewWrite for DirectReview {
    fn add_comment(&mut self, comment: Comment) -> CommentId {
        let id = comment.id;
        self.comments.push(comment);
        id
    }

    fn resolve_comment(&mut self, id: CommentId) -> anyhow::Result<()> {
        let comment = self
            .comments
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow::anyhow!("commentaire introuvable : {id}"))?;
        comment.resolved = true;
        Ok(())
    }

    fn delete_comment(&mut self, id: CommentId) -> anyhow::Result<()> {
        let len_before = self.comments.len();
        self.comments.retain(|c| c.id != id);
        if self.comments.len() == len_before {
            bail!("commentaire introuvable : {id}");
        }
        Ok(())
    }

    fn add_work_note(&mut self, note: WorkNote) -> CommentId {
        let id = note.id;
        self.work_notes.push(note);
        id
    }

    fn delete_work_note(&mut self, id: CommentId) -> anyhow::Result<()> {
        let len_before = self.work_notes.len();
        self.work_notes.retain(|n| n.id != id);
        if self.work_notes.len() == len_before {
            bail!("note de travail introuvable : {id}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_resolve_comment() {
        let mut review = DirectReview::new();
        let id = review.add_comment(Comment::new("alice", "à revoir"));
        assert!(!review.comment(id).unwrap().resolved);
        review.resolve_comment(id).unwrap();
        assert!(review.comment(id).unwrap().resolved);
    }

    #[test]
    fn test_delete_thread_removes_replies() {
        let mut review = DirectReview::new();
        let root = review.add_comment(Comment::new("alice", "root"));
        let reply = review.add_comment(Comment::new("bob", "reply").reply_to(root));
        review.delete_comment_thread(root).unwrap();
        assert!(review.comment(root).is_none());
        assert!(review.comment(reply).is_none());
    }

    #[test]
    fn test_try_delete_comment_requires_author() {
        let mut review = DirectReview::new();
        let id = review.add_comment(Comment::new("alice", "texte"));
        assert!(review.try_delete_comment(id, "bob").is_err());
        assert!(review.comment(id).is_some());
        assert!(review.try_delete_comment(id, "alice").is_ok());
        assert!(review.comment(id).is_none());
    }

    #[test]
    fn test_try_resolve_comment_allows_author_or_editor() {
        let mut review = DirectReview::new();
        let id = review.add_comment(Comment::new("alice", "texte"));
        assert!(review.try_resolve_comment(id, "bob", false).is_err());
        assert!(review.try_resolve_comment(id, "bob", true).is_ok());
    }

    #[test]
    fn test_root_comments_and_replies() {
        let mut review = DirectReview::new();
        let root = review.add_comment(Comment::new("alice", "root"));
        review.add_comment(Comment::new("bob", "reply").reply_to(root));
        assert_eq!(review.root_comments().len(), 1);
        assert_eq!(review.replies_to(root).len(), 1);
    }
}
