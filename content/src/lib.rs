//! Modèle `Content` (arbre rich-text) et abstraction permettant de le
//! manipuler aussi bien en mode direct (mémoire locale) qu'en mode Yrs
//! (CRDT collaboratif), via les traits [`ContentRead`]/[`ContentWrite`] et
//! le handle opaque [`ContentHandle`].

mod crdt;
mod cursor;
mod direct;
pub mod editor;
mod handle;
mod id;
mod kind;
mod traits;

pub use crdt::YrsContent;
pub use cursor::Cursor;
pub use direct::DirectContent;
pub use editor::ContentEditor;
pub use handle::ContentHandle;
pub use id::ContentId;
pub use kind::{
    Cell, ContentKind, List, ListItem, ListMarker, NodeSpec, Paragraph, Row, Span, Table,
};
pub use traits::{ContentRead, ContentWrite};
