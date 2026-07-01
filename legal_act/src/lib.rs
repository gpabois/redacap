//! Modèle du corps d'un acte légal (arrêté préfectoral ICPE) et
//! abstraction permettant de le manipuler en mode direct (mémoire locale)
//! ou en mode Yrs (CRDT collaboratif), via les traits [`BodyRead`] /
//! [`BodyWrite`] et les backends [`DirectBody`] / [`YrsBody`].

mod body;
mod crdt;
mod cursor;
mod direct;
pub mod editor;
mod id;
mod kind;
pub mod traits;

pub use body::Body;
pub use crdt::YrsBody;
pub use cursor::{Cursor, Selection};
pub use direct::DirectBody;
pub use editor::LegalActEditor;
pub use id::BodyNodeId;
pub use kind::{
    Annexe, Article, Chapitre, NodeKind, NodeSpec, Section, Titre,
};
pub use traits::node::{BodyRead, BodyWrite};
pub use traits::{
    LegalActId, LegalActKind, LegalActMeta, LegalActRead, LegalActWrite, ProjectId, ProjectMeta,
    ProjectRead, ProjectStatus, ProjectWrite, Comment, CommentId, ReviewRead, ReviewWrite, WorkNote,
};
