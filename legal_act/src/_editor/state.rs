use crate::cursor::{Cursor, Selection};

/// Amorce d'un commentaire en cours de composition dans le panneau latéral
/// (voir [`super::context::EditorContext::pending_comment`] et
/// [`super::review::ReviewPanel`]) : `selection`/`excerpt` sont renseignés
/// quand le commentaire est ancré à un extrait de texte (voir le bouton
/// "Commenter" de [`super::header::ContentToolbar`]), `None` pour un
/// commentaire général.
#[derive(Debug, Clone, Default)]
pub struct PendingComment {
    pub selection: Option<Selection>,
    pub excerpt: Option<String>,
}

/// Identifiant d'un curseur collaboratif (un par pair connecté).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CursorId(pub u64);

/// Curseur de l'éditeur d'acte légal (caret + souris).
#[derive(Debug, Clone, Copy)]
pub struct EditorCursor {
    pub id: CursorId,
    pub caret: Cursor,
    pub mouse: Cursor,
    /// Le caret est-il visible (focus actif) ?
    pub display: bool,
}

/// État de la sélection de l'éditeur.
#[derive(Debug, Clone, Default)]
pub struct EditorSelection {
    pub state: SelectionState,
    pub anchor: Option<Cursor>,
    pub focus: Option<Cursor>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SelectionState {
    #[default]
    Idle,
    Dragging,
}

impl EditorSelection {
    pub fn selection(&self) -> Option<Selection> {
        Some(Selection {
            anchor: self.anchor?,
            focus: self.focus?,
        })
    }

    /// Corrige l'ordre anchor ≤ focus dans le document.
    pub fn correct<B: crate::traits::node::BodyAccess + ?Sized>(&mut self, body: &B) {
        if let (Some(anchor), Some(focus)) = (self.anchor, self.focus) {
            let mut sel = Selection { anchor, focus };
            sel.correct(body);
            self.anchor = Some(sel.anchor);
            self.focus = Some(sel.focus);
        }
    }
}
