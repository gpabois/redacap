//! Modèle du corps d'un acte légal (arrêté préfectoral ICPE) et
//! abstraction permettant de le manipuler en mode direct (mémoire locale)
//! ou en mode Yrs (CRDT collaboratif), via les traits [`BodyRead`] /
//! [`BodyWrite`] et les backends [`DirectBody`] / [`YrsBody`].

// Les vues Leptos imbriquées de l'éditeur (voir `editor::component`) génèrent
// des types de composants profondément imbriqués ; la limite par défaut du
// vérificateur de types est dépassée (voir aussi `server/src/main.rs`).
#![recursion_limit = "256"]

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
pub use editor::{ConnectedUser, LegalActEditor};
pub use id::BodyNodeId;
pub use kind::{Annexe, Article, Chapitre, NodeKind, NodeSpec, Section, Titre};
pub use traits::node::{BodyRead, BodyWrite};
pub use traits::{
    Comment, CommentId, LegalActId, LegalActKind, LegalActMeta, LegalActRead, LegalActWrite,
    ProjectId, ProjectMeta, ProjectRead, ProjectStatus, ProjectWrite, ReviewRead, ReviewWrite,
    WorkNote,
};
