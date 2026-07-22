//! Rendition du corps d'un acte légal ([`legal_act::traits::node::BodyAccess`])
//! en contenu `<office:text>` d'un `content.xml` ODF.

use std::borrow::Cow;
use std::io::Write;

use content::{ListMarker, Span};
use legal_act::{NodeId, BodyAccess, NodeKind, NodeSpec};
use quick_xml::Writer;

use crate::error::RenderError;
use crate::odt::numbering::{list_style_name, to_roman};
use crate::odt::style_names as s;
use crate::odt::xml::{write_element, write_text_run};

/// Génère le contenu de `<office:text>` pour l'ensemble du corps de l'acte.
/// `first_page_header` reflète la présence d'un en-tête bloc-marque (voir
/// [`crate::odt::styles`]) : si `true`, le tout premier paragraphe du corps
/// (toujours un `Visa`/`Considerant`/`Sur`/subdivision/`Article`, cf. les
/// invariants structurels de `Root`) reçoit le style
/// [`first_page_style_name`] pour démarrer sur le master-page
/// [`s::FIRST_PAGE_MASTER`] plutôt que `Standard`.
pub(crate) fn render_office_text<W: Write, B: BodyAccess>(
    writer: &mut Writer<W>,
    body: &B,
    first_page_header: bool,
) -> Result<(), RenderError> {
    let mut children = body.children_of(body.root()).into_iter();
    if let Some(first) = children.next() {
        render_node(body, first, writer, first_page_header)?;
    }
    for child in children {
        render_node(body, child, writer, false)?;
    }
    Ok(())
}

/// Styles de paragraphe automatiques déclarés dans `content.xml`, utilisés
/// uniquement par le tout premier paragraphe du corps pour le faire démarrer
/// sur [`s::FIRST_PAGE_MASTER`] (voir [`render_office_text`]).
pub(crate) fn write_first_page_styles<W: Write>(writer: &mut Writer<W>) -> Result<(), RenderError> {
    write_element(writer, "office:automatic-styles", &[], |writer| {
        for base in [
            s::VISA,
            s::CONSIDERANT,
            s::SUR,
            s::TITRE,
            s::CHAPITRE,
            s::SECTION,
            s::ARTICLE,
            s::ANNEXE,
        ] {
            let name = first_page_style_name(base);
            write_element(
                writer,
                "style:style",
                &[
                    ("style:name", name.as_ref()),
                    ("style:family", "paragraph"),
                    ("style:parent-style-name", base),
                    ("style:master-page-name", s::FIRST_PAGE_MASTER),
                ],
                |_| Ok(()),
            )?;
        }
        Ok(())
    })
}

/// Nom du style de paragraphe "première page" dérivé d'un style de base
/// (doit rester en phase avec [`write_first_page_styles`]).
fn first_page_style_name(base: &str) -> String {
    format!("{base}_FirstPage")
}

/// Résout le nom de style à appliquer à un paragraphe de tête, en tenant
/// compte de l'éventuel override "première page" (voir
/// [`render_office_text`]).
fn resolve_style(base: &'static str, first_page: bool) -> Cow<'static, str> {
    if first_page {
        Cow::Owned(first_page_style_name(base))
    } else {
        Cow::Borrowed(base)
    }
}

/// Style de caractère ODF portant les attributs d'un [`Span`]. Le nom doit
/// rester en phase avec les styles déclarés par
/// [`crate::odt::styles::write_character_styles`].
fn span_style_name(span: &Span) -> String {
    format!(
        "Span_{}{}{}{}",
        span.bold as u8, span.italic as u8, span.underline as u8, span.strikeout as u8
    )
}

fn render_node<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    writer: &mut Writer<W>,
    first_page: bool,
) -> Result<(), RenderError> {
    match body.spec_of(id) {
        NodeSpec::Visa => {
            render_paragraph_like(body, id, &resolve_style(s::VISA, first_page), writer)
        }
        NodeSpec::Considerant => {
            render_paragraph_like(body, id, &resolve_style(s::CONSIDERANT, first_page), writer)
        }
        NodeSpec::Sur => {
            render_paragraph_like(body, id, &resolve_style(s::SUR, first_page), writer)
        }
        NodeSpec::Titre(t) => render_subdivision(
            body,
            id,
            &resolve_style(s::TITRE, first_page),
            &format!("Titre {}", to_roman(t.number)),
            NodeKind::LibelleTitre,
            writer,
        ),
        NodeSpec::Chapitre(c) => render_subdivision(
            body,
            id,
            &resolve_style(s::CHAPITRE, first_page),
            &format!("Chapitre {}", c.number),
            NodeKind::LibelleChapitre,
            writer,
        ),
        NodeSpec::Section(sec) => render_subdivision(
            body,
            id,
            &resolve_style(s::SECTION, first_page),
            &format!("Section {}", sec.number),
            NodeKind::LibelleSection,
            writer,
        ),
        NodeSpec::Article(a) => render_article(
            body,
            id,
            a.number,
            &resolve_style(s::ARTICLE, first_page),
            writer,
        ),
        NodeSpec::Annexe(a) => render_subdivision(
            body,
            id,
            &resolve_style(s::ANNEXE, first_page),
            &format!("Annexe {}", to_roman(a.number)),
            NodeKind::LibelleAnnexe,
            writer,
        ),
        NodeSpec::Paragraphe => {
            render_paragraph_like(body, id, &resolve_style(s::PARAGRAPHE, first_page), writer)
        }
        NodeSpec::Table => render_table(body, id, writer),
        NodeSpec::List(list) => render_list(body, id, list.marker, list.start, writer),
        _ => Err(RenderError::UnexpectedNode(body.kind_of(id))),
    }
}

/// Rend un paragraphe dont les enfants directs sont des nœuds de contenu
/// en ligne (`Plain`/`Span`) : `Visa`, `Considerant`, `Sur`, `Paragraphe`.
fn render_paragraph_like<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    style: &str,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    write_element(writer, "text:p", &[("text:style-name", style)], |writer| {
        render_inline_children(body, id, writer)
    })
}

/// Rend une subdivision numérotée (Titre/Chapitre/Section/Annexe) : un
/// paragraphe d'en-tête suivi de ses enfants structurels, dans l'ordre.
fn render_subdivision<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    style: &str,
    heading: &str,
    label_kind: NodeKind,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    write_element(writer, "text:p", &[("text:style-name", style)], |writer| {
        write_text_run(writer, heading)?;
        if let Some(label) = body
            .children_of(id)
            .into_iter()
            .find(|&c| body.kind_of(c) == label_kind)
        {
            write_text_run(writer, " — ")?;
            render_inline_children(body, label, writer)?;
        }
        Ok(())
    })?;

    for child in body.children_of(id) {
        if body.kind_of(child) != label_kind {
            render_node(body, child, writer, false)?;
        }
    }
    Ok(())
}

/// Rend un article : en-tête numéroté puis contenu de son `ArticleBody`
/// (paragraphes, tableaux, listes).
fn render_article<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    number: u32,
    style: &str,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    write_element(writer, "text:p", &[("text:style-name", style)], |writer| {
        write_text_run(writer, &format!("Article {number}"))?;
        if let Some(label) = body
            .children_of(id)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::LibelleArticle)
        {
            write_text_run(writer, " — ")?;
            render_inline_children(body, label, writer)?;
        }
        Ok(())
    })?;

    if let Some(article_body) = body
        .children_of(id)
        .into_iter()
        .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
    {
        for child in body.children_of(article_body) {
            render_node(body, child, writer, false)?;
        }
    }
    Ok(())
}

fn render_table<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    let rows = body.children_of(id);
    let columns = rows
        .iter()
        .map(|&row| body.children_of(row).len())
        .max()
        .unwrap_or(1)
        .max(1)
        .to_string();
    let name = format!("Table_{id}");

    write_element(
        writer,
        "table:table",
        &[("table:name", &name), ("table:style-name", s::TABLE)],
        |writer| {
            writer
                .create_element("table:table-column")
                .with_attribute(("table:style-name", s::TABLE_COLUMN))
                .with_attribute(("table:number-columns-repeated", columns.as_str()))
                .write_empty()?;
            for row in rows {
                render_table_row(body, row, writer)?;
            }
            Ok(())
        },
    )
}

fn render_table_row<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    write_element(
        writer,
        "table:table-row",
        &[("table:style-name", s::TABLE_ROW)],
        |writer| {
            for cell in body.children_of(id) {
                render_table_cell(body, cell, writer)?;
            }
            Ok(())
        },
    )
}

fn render_table_cell<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    write_element(
        writer,
        "table:table-cell",
        &[
            ("table:style-name", s::TABLE_CELL),
            ("office:value-type", "string"),
        ],
        |writer| {
            for child in body.children_of(id) {
                match body.kind_of(child) {
                    NodeKind::Paragraphe => {
                        render_paragraph_like(body, child, s::TABLE_PARAGRAPH, writer)?
                    }
                    NodeKind::List => {
                        if let NodeSpec::List(list) = body.spec_of(child) {
                            render_list(body, child, list.marker, list.start, writer)?;
                        }
                    }
                    other => return Err(RenderError::UnexpectedNode(other)),
                }
            }
            Ok(())
        },
    )
}

fn render_list<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    marker: ListMarker,
    start: Option<u32>,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    write_element(
        writer,
        "text:list",
        &[("text:style-name", list_style_name(marker))],
        |writer| {
            for (index, item) in body.children_of(id).into_iter().enumerate() {
                let start_value = start.filter(|_| index == 0).map(|n| n.to_string());
                let attrs: &[(&str, &str)] = match &start_value {
                    Some(n) => &[("text:start-value", n.as_str())],
                    None => &[],
                };
                write_element(writer, "text:list-item", attrs, |writer| {
                    write_element(
                        writer,
                        "text:p",
                        &[("text:style-name", s::LIST_PARAGRAPH)],
                        |writer| render_inline_children(body, item, writer),
                    )
                })?;
            }
            Ok(())
        },
    )
}

fn render_inline_children<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    for child in body.children_of(id) {
        render_inline(body, child, writer)?;
    }
    Ok(())
}

fn render_inline<W: Write, B: BodyAccess>(
    body: &B,
    id: NodeId,
    writer: &mut Writer<W>,
) -> Result<(), RenderError> {
    match body.spec_of(id) {
        NodeSpec::Plain(text) => write_text_run(writer, &text)?,
        NodeSpec::Span(span) => {
            let style_name = span_style_name(&span);
            write_element(
                writer,
                "text:span",
                &[("text:style-name", &style_name)],
                |writer| render_inline_children(body, id, writer),
            )?;
        }
        _ => return Err(RenderError::UnexpectedNode(body.kind_of(id))),
    }
    Ok(())
}
