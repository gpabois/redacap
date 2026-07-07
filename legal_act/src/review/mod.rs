//! Stockage des commentaires et notes de travail d'un projet d'acte légal,
//! avec la même dualité de backend que [`crate::Body`] : mode direct
//! (mémoire locale) ou mode Yrs (CRDT collaboratif), cachée derrière l'API
//! opaque [`Review`].

mod crdt;
mod direct;

pub use crdt::YrsReview;
pub use direct::DirectReview;

use crate::traits::review::{Comment, CommentId, ReviewRead, ReviewWrite, WorkNote};

/// Abstraction sur le backend de stockage des commentaires et notes de
/// travail d'un projet. Permet d'utiliser indifféremment le mode direct ou
/// le mode Yrs dans les composants Leptos, qui n'ont pas à savoir dans
/// quel mode ils opèrent (voir [`crate::Body`] pour le pendant côté corps
/// de l'acte).
pub enum Review {
    Direct(DirectReview),
    Yrs(YrsReview),
}

impl Review {
    pub fn direct() -> Self {
        Self::Direct(DirectReview::new())
    }

    pub fn yrs() -> Self {
        Self::Yrs(YrsReview::new())
    }
}

impl Default for Review {
    fn default() -> Self {
        Self::direct()
    }
}

impl From<DirectReview> for Review {
    fn from(value: DirectReview) -> Self {
        Self::Direct(value)
    }
}

impl From<YrsReview> for Review {
    fn from(value: YrsReview) -> Self {
        Self::Yrs(value)
    }
}

impl ReviewRead for Review {
    fn comments(&self) -> Vec<Comment> {
        match self {
            Self::Direct(r) => r.comments(),
            Self::Yrs(r) => r.comments(),
        }
    }

    fn work_notes(&self) -> Vec<WorkNote> {
        match self {
            Self::Direct(r) => r.work_notes(),
            Self::Yrs(r) => r.work_notes(),
        }
    }
}

impl ReviewWrite for Review {
    fn add_comment(&mut self, comment: Comment) -> CommentId {
        match self {
            Self::Direct(r) => r.add_comment(comment),
            Self::Yrs(r) => r.add_comment(comment),
        }
    }

    fn resolve_comment(&mut self, id: CommentId) -> anyhow::Result<()> {
        match self {
            Self::Direct(r) => r.resolve_comment(id),
            Self::Yrs(r) => r.resolve_comment(id),
        }
    }

    fn delete_comment(&mut self, id: CommentId) -> anyhow::Result<()> {
        match self {
            Self::Direct(r) => r.delete_comment(id),
            Self::Yrs(r) => r.delete_comment(id),
        }
    }

    fn add_work_note(&mut self, note: WorkNote) -> CommentId {
        match self {
            Self::Direct(r) => r.add_work_note(note),
            Self::Yrs(r) => r.add_work_note(note),
        }
    }

    fn delete_work_note(&mut self, id: CommentId) -> anyhow::Result<()> {
        match self {
            Self::Direct(r) => r.delete_work_note(id),
            Self::Yrs(r) => r.delete_work_note(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(mut review: Review) {
        let id = review.add_comment(Comment::new("alice", "hello"));
        assert_eq!(review.comments().len(), 1);
        review.resolve_comment(id).unwrap();
        assert!(review.comment(id).unwrap().resolved);
    }

    #[test]
    fn test_both_backends_through_the_opaque_handle() {
        exercise(Review::direct());
        exercise(Review::yrs());
    }
}
