//! Rendition ODT : un acte légal ([`legal_act::LegalActRead`]) est
//! converti en archive ODF (`content.xml`, `styles.xml`, `meta.xml`,
//! `mimetype`) sans aucun accès disque ou réseau — l'archive est
//! entièrement construite en mémoire et renvoyée sous forme d'octets.

mod content;
mod manifest;
mod meta;
mod numbering;
mod package;
mod style_names;
mod styles;
mod xml;

pub(crate) use package::build;
