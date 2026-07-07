//! Conversion de la sélection navigateur (DOM) courante vers une
//! [`Selection`] du domaine, pour ancrer un commentaire à un extrait de
//! texte (voir [`super::header::ContentToolbar`]'s bouton "Commenter").
//!
//! Repose sur le marquage `data-plain-id="<id>"` posé par
//! `super::content::node_to_inline_html` sur chaque feuille `Plain` rendue :
//! une portion de texte sélectionnée par l'utilisateur peut ainsi toujours
//! être retracée jusqu'au nœud `Plain` et à l'offset (en caractères Unicode)
//! qui la porte dans le corps de l'acte, indépendamment de tout état
//! intermédiaire non encore synchronisé dans [`crate::Body`].

use leptos::prelude::document;
use web_sys::Element;
use web_sys::wasm_bindgen::JsCast;

use crate::cursor::{Cursor, Selection};
use crate::traits::node::BodyRead;
use crate::BodyNodeId;

/// Capture la sélection navigateur courante si elle est non vide et
/// entièrement comprise dans `boundary` (la racine DOM du
/// `RichEditableDiv` focus), et la convertit en [`Selection`] du domaine +
/// extrait de texte figé. `None` si la sélection est vide/collapsée, hors
/// de `boundary`, ou si le mapping échoue (nœud `data-plain-id` introuvable).
pub(super) fn capture_content_selection(
    boundary: &Element,
    body: &impl BodyRead,
) -> Option<(Selection, String)> {
    let sel = document().get_selection().ok()??;
    if sel.is_collapsed() || sel.range_count() == 0 {
        return None;
    }
    let range = sel.get_range_at(0).ok()?;

    let start_container = range.start_container().ok()?;
    let end_container = range.end_container().ok()?;
    if !boundary.contains(Some(&start_container)) || !boundary.contains(Some(&end_container)) {
        return None;
    }

    let anchor = cursor_at_start(&start_container, range.start_offset().ok()?)?;
    let focus = cursor_at_end(&end_container, range.end_offset().ok()?)?;

    let selection = Selection { anchor, focus };
    let excerpt = selection.extract_text(body);
    if excerpt.is_empty() {
        return None;
    }
    Some((selection, excerpt))
}

/// Résout la borne de début d'une sélection : nœud texte (cas courant, on
/// convertit l'offset UTF-16 du DOM en offset caractère) ou nœud élément
/// (cas limite, ex. double-clic en bord de mot : on prend le début de la
/// première feuille `Plain` rencontrée dans le sous-arbre).
fn cursor_at_start(container: &web_sys::Node, offset: u32) -> Option<Cursor> {
    if container.node_type() == web_sys::Node::TEXT_NODE {
        return cursor_in_text_node(container, offset);
    }
    let el = first_plain_descendant(container)?;
    let plain_id: BodyNodeId = el.get_attribute("data-plain-id")?.parse().ok()?;
    Some(Cursor {
        node_id: plain_id,
        offset: 0,
    })
}

/// Pendant de [`cursor_at_start`] pour la borne de fin : nœud élément → fin
/// de la dernière feuille `Plain` rencontrée.
fn cursor_at_end(container: &web_sys::Node, offset: u32) -> Option<Cursor> {
    if container.node_type() == web_sys::Node::TEXT_NODE {
        return cursor_in_text_node(container, offset);
    }
    let el = last_plain_descendant(container)?;
    let plain_id: BodyNodeId = el.get_attribute("data-plain-id")?.parse().ok()?;
    let full_len = el.text_content().unwrap_or_default().chars().count();
    Some(Cursor {
        node_id: plain_id,
        offset: full_len,
    })
}

fn cursor_in_text_node(text_node: &web_sys::Node, utf16_offset: u32) -> Option<Cursor> {
    let parent = text_node.parent_element()?;
    let plain_id: BodyNodeId = parent.get_attribute("data-plain-id")?.parse().ok()?;
    let text = text_node.text_content().unwrap_or_default();
    Some(Cursor {
        node_id: plain_id,
        offset: utf16_offset_to_char_offset(&text, utf16_offset as usize),
    })
}

fn utf16_offset_to_char_offset(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0usize;
    for (char_idx, ch) in text.chars().enumerate() {
        if units >= utf16_offset {
            return char_idx;
        }
        units += ch.len_utf16();
    }
    text.chars().count()
}

fn first_plain_descendant(node: &web_sys::Node) -> Option<Element> {
    if let Some(el) = node.dyn_ref::<Element>()
        && el.has_attribute("data-plain-id")
    {
        return Some(el.clone());
    }
    let mut child = node.first_child();
    while let Some(c) = child {
        if let Some(found) = first_plain_descendant(&c) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

fn last_plain_descendant(node: &web_sys::Node) -> Option<Element> {
    if let Some(el) = node.dyn_ref::<Element>()
        && el.has_attribute("data-plain-id")
    {
        return Some(el.clone());
    }
    let mut child = node.last_child();
    while let Some(c) = child {
        if let Some(found) = last_plain_descendant(&c) {
            return Some(found);
        }
        child = c.previous_sibling();
    }
    None
}
