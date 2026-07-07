use std::str::FromStr;

use anyhow::{anyhow, bail};

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

impl std::fmt::Display for CommentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for CommentId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

/// Commentaire associé (ou non) à une sélection dans le corps de l'acte.
/// Peut être répondu de manière arborescente via [`Self::parent`].
#[derive(Debug, Clone)]
pub struct Comment {
    pub id: CommentId,
    pub author: String,
    pub text: String,
    /// Sélection dans le corps de l'acte à laquelle ce commentaire est
    /// ancré ; `None` pour un commentaire général, non rattaché à un extrait
    /// (voir exigence : un commentaire peut sélectionner une partie de
    /// l'arrêté ou non).
    pub selection: Option<Selection>,
    /// Extrait de texte recouvert par [`Self::selection`] au moment de la
    /// création du commentaire, figé indépendamment des modifications
    /// ultérieures du corps (voir exigence : « les commentaires reprennent
    /// dans la bulle l'extrait sélectionné »).
    pub excerpt: Option<String>,
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
            excerpt: None,
            parent: None,
            resolved: false,
        }
    }

    /// Ancre le commentaire à `selection`, en conservant `excerpt` (le texte
    /// recouvert au moment de la création) pour l'affichage dans la bulle.
    #[must_use]
    pub fn with_selection(mut self, selection: Selection, excerpt: impl Into<String>) -> Self {
        self.selection = Some(selection);
        self.excerpt = Some(excerpt.into());
        self
    }

    /// Marque ce commentaire comme réponse à `parent`.
    #[must_use]
    pub fn reply_to(mut self, parent: CommentId) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn is_reply(&self) -> bool {
        self.parent.is_some()
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

impl WorkNote {
    pub fn new(author: impl Into<String>, text: impl Into<String>, private: bool) -> Self {
        Self {
            id: CommentId::new(),
            author: author.into(),
            text: text.into(),
            private,
        }
    }
}

/// Trait de lecture des commentaires et notes d'un projet, quel que soit le
/// backend (mémoire directe ou `yrs`) : voir [`crate::DirectReview`] /
/// [`crate::YrsReview`] et l'API opaque [`crate::Review`].
pub trait ReviewRead {
    fn comments(&self) -> Vec<Comment>;
    fn work_notes(&self) -> Vec<WorkNote>;

    fn comment(&self, id: CommentId) -> Option<Comment> {
        self.comments().into_iter().find(|c| c.id == id)
    }

    /// Commentaires racines (non-réponses), dans l'ordre de création.
    fn root_comments(&self) -> Vec<Comment> {
        self.comments()
            .into_iter()
            .filter(|c| c.parent.is_none())
            .collect()
    }

    /// Réponses directes à `id`, dans l'ordre de création.
    fn replies_to(&self, id: CommentId) -> Vec<Comment> {
        self.comments()
            .into_iter()
            .filter(|c| c.parent == Some(id))
            .collect()
    }
}

/// Trait d'écriture des commentaires et notes d'un projet. Étend [`ReviewRead`].
pub trait ReviewWrite: ReviewRead {
    // ── Primitives ────────────────────────────────────────────────────────

    fn add_comment(&mut self, comment: Comment) -> CommentId;
    /// Marque un unique commentaire comme résolu, sans vérification de droits.
    fn resolve_comment(&mut self, id: CommentId) -> anyhow::Result<()>;
    /// Supprime un unique commentaire (pas ses réponses), sans vérification
    /// de droits. Voir [`Self::delete_comment_thread`] pour la suppression
    /// en cascade.
    fn delete_comment(&mut self, id: CommentId) -> anyhow::Result<()>;
    fn add_work_note(&mut self, note: WorkNote) -> CommentId;
    fn delete_work_note(&mut self, id: CommentId) -> anyhow::Result<()>;

    // ── Dérivées ─────────────────────────────────────────────────────────

    /// Supprime `id` ainsi que toutes ses réponses, récursivement.
    fn delete_comment_thread(&mut self, id: CommentId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let mut stack = vec![id];
        let mut to_delete = vec![];
        while let Some(current) = stack.pop() {
            stack.extend(self.replies_to(current).into_iter().map(|c| c.id));
            to_delete.push(current);
        }
        for cid in to_delete {
            self.delete_comment(cid)?;
        }
        Ok(())
    }

    /// Supprime le commentaire `id` (et ses réponses) après vérification que
    /// `actor` en est bien l'auteur : seul l'auteur peut supprimer son
    /// commentaire.
    fn try_delete_comment(&mut self, id: CommentId, actor: &str) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let comment = self
            .comment(id)
            .ok_or_else(|| anyhow!("commentaire introuvable : {id}"))?;
        if comment.author != actor {
            bail!("seul l'auteur du commentaire peut le supprimer");
        }
        self.delete_comment_thread(id)
    }

    /// Résout le commentaire `id` après vérification que `actor` en est
    /// l'auteur ou dispose des droits d'édition du projet
    /// (`actor_can_edit`) : un commentaire peut être résolu par son auteur
    /// ou par un rédacteur.
    fn try_resolve_comment(
        &mut self,
        id: CommentId,
        actor: &str,
        actor_can_edit: bool,
    ) -> anyhow::Result<()> {
        let comment = self
            .comment(id)
            .ok_or_else(|| anyhow!("commentaire introuvable : {id}"))?;
        if comment.author != actor && !actor_can_edit {
            bail!("seul l'auteur ou un rédacteur peut résoudre ce commentaire");
        }
        self.resolve_comment(id)
    }
}
