use crate::cursor::Selection;

/// Identifiant d'un commentaire.
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct CommentId(shared::id::ID);

impl CommentId {
    pub fn new() -> Self {
        Self(shared::id::generate_id())
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Commentaire associé à une sélection dans le corps de l'acte.
#[derive(Debug, Clone)]
pub struct Comment {
    pub id: CommentId,
    pub author: String,
    pub text: String,
    /// Sélection dans le corps de l'acte à laquelle ce commentaire est ancré.
    pub selection: Option<Selection>,
    /// Commentaire parent (pour les réponses arborescentes).
    pub parent: Option<CommentId>,
    pub resolved: bool,
}

impl Comment {
    pub fn new(author: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: CommentId::new(),
            author: author.into(),
            text: text.into(),
            selection: None,
            parent: None,
            resolved: false,
        }
    }
}

/// Note de travail (peut être privée ou publique).
#[derive(Debug, Clone)]
pub struct WorkNote {
    pub id: CommentId,
    pub author: String,
    pub text: String,
    pub private: bool,
}

/// Trait de lecture des commentaires et notes d'un projet.
pub trait ReviewRead {
    fn comments(&self) -> Vec<&Comment>;
    fn root_comments(&self) -> Vec<&Comment> {
        self.comments()
            .into_iter()
            .filter(|c| c.parent.is_none())
            .collect()
    }
    fn replies_to(&self, id: CommentId) -> Vec<&Comment> {
        self.comments()
            .into_iter()
            .filter(|c| c.parent == Some(id))
            .collect()
    }
    fn work_notes(&self) -> Vec<&WorkNote>;
}

/// Trait d'écriture des commentaires et notes d'un projet.
pub trait ReviewWrite: ReviewRead {
    fn add_comment(&mut self, comment: Comment) -> CommentId;
    fn resolve_comment(&mut self, id: CommentId) -> anyhow::Result<()>;
    fn delete_comment(&mut self, id: CommentId) -> anyhow::Result<()>;
    fn add_work_note(&mut self, note: WorkNote) -> CommentId;
    fn delete_work_note(&mut self, id: CommentId) -> anyhow::Result<()>;
}
