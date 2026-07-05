//! Génération de `styles.xml` : styles communs (paragraphe, caractère,
//! liste, tableau) et mise en page de base, partagés par tous les actes.
//! Le contenu est statique à l'exception des styles de caractère, dérivés
//! par combinatoire des attributs de [`content::Span`].

use std::io::Write;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, Event};

use crate::error::RenderError;
use crate::odt::style_names as s;
use crate::odt::xml::{write_element, write_text_run};

/// Génère `styles.xml`. `authority_name`/`issuer_name` proviennent de
/// [`legal_act::LegalActMeta`] : dès que l'un des deux est renseigné, la
/// première page reçoit un en-tête avec le bloc-marque Marianne à gauche
/// (« RÉPUBLIQUE FRANÇAISE » + `authority_name`) et `issuer_name` à droite ;
/// les pages suivantes reviennent au master-page `Standard`, sans en-tête.
pub(crate) fn build(
    authority_name: Option<&str>,
    issuer_name: Option<&str>,
) -> Result<Vec<u8>, RenderError> {
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    write_element(
        &mut writer,
        "office:document-styles",
        &[
            (
                "xmlns:office",
                "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
            ),
            (
                "xmlns:style",
                "urn:oasis:names:tc:opendocument:xmlns:style:1.0",
            ),
            (
                "xmlns:text",
                "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
            ),
            (
                "xmlns:table",
                "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
            ),
            (
                "xmlns:fo",
                "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
            ),
            ("office:version", "1.3"),
        ],
        |writer| {
            write_office_styles(writer)?;
            write_automatic_styles(writer)?;
            write_master_styles(writer, authority_name, issuer_name)
        },
    )?;

    Ok(writer.into_inner())
}

/// Styles de paragraphe, de caractère et de liste partagés par le corps de
/// tous les actes.
fn write_office_styles<W: Write>(writer: &mut Writer<W>) -> Result<(), RenderError> {
    write_element(writer, "office:styles", &[], |writer| {
        write_element(
            writer,
            "style:default-style",
            &[("style:family", "paragraph")],
            |writer| {
                writer
                    .create_element("style:paragraph-properties")
                    .with_attribute(("fo:text-align", "justify"))
                    .write_empty()?;
                writer
                    .create_element("style:text-properties")
                    .with_attribute(("style:font-name", "Marianne"))
                    .with_attribute(("fo:font-size", "12pt"))
                    .with_attribute(("fo:font-align", "justify"))
                    .with_attribute(("fo:language", "fr"))
                    .with_attribute(("fo:country", "FR"))
                    .write_empty()?;
                Ok(())
            },
        )?;
        writer
            .create_element("style:style")
            .with_attribute(("style:name", "Standard"))
            .with_attribute(("style:family", "paragraph"))
            .with_attribute(("style:class", "text"))
            .write_empty()?;

        paragraph_style(
            writer,
            s::VISA,
            &[("fo:margin-bottom", "0.1cm")],
            &[("fo:font-style", "italic")],
        )?;
        paragraph_style(
            writer,
            s::CONSIDERANT,
            &[("fo:margin-bottom", "0.1cm")],
            &[("fo:font-style", "italic")],
        )?;
        paragraph_style(
            writer,
            s::SUR,
            &[("fo:margin-top", "0.2cm"), ("fo:margin-bottom", "0.4cm")],
            &[("fo:font-style", "italic")],
        )?;
        paragraph_style(
            writer,
            s::TITRE,
            &[
                ("fo:text-align", "center"),
                ("fo:margin-top", "0.6cm"),
                ("fo:margin-bottom", "0.3cm"),
                ("fo:break-before", "page"),
            ],
            &[
                ("fo:font-size", "15pt"),
                ("fo:font-weight", "bold"),
                ("fo:text-transform", "uppercase"),
            ],
        )?;
        paragraph_style(
            writer,
            s::CHAPITRE,
            &[("fo:margin-top", "0.4cm"), ("fo:margin-bottom", "0.2cm")],
            &[("fo:font-size", "13pt"), ("fo:font-weight", "bold")],
        )?;
        paragraph_style(
            writer,
            s::SECTION,
            &[("fo:margin-top", "0.3cm"), ("fo:margin-bottom", "0.2cm")],
            &[
                ("fo:font-size", "12pt"),
                ("fo:font-weight", "bold"),
                ("fo:font-style", "italic"),
            ],
        )?;
        paragraph_style(
            writer,
            s::ARTICLE,
            &[("fo:margin-top", "0.3cm"), ("fo:margin-bottom", "0.2cm")],
            &[("fo:font-size", "12pt"), ("fo:font-weight", "bold")],
        )?;
        paragraph_style(
            writer,
            s::ANNEXE,
            &[
                ("fo:text-align", "center"),
                ("fo:margin-top", "0.6cm"),
                ("fo:margin-bottom", "0.3cm"),
                ("fo:break-before", "page"),
            ],
            &[
                ("fo:font-size", "15pt"),
                ("fo:font-weight", "bold"),
                ("fo:text-transform", "uppercase"),
            ],
        )?;
        paragraph_style(writer, s::PARAGRAPHE, &[("fo:margin-bottom", "0.2cm")], &[])?;
        paragraph_style(
            writer,
            s::HEADER_BLOC_MARQUE,
            &[("fo:text-align", "start"), ("fo:margin", "0cm")],
            &[("fo:font-weight", "bold"), ("fo:font-size", "10pt")],
        )?;
        paragraph_style(
            writer,
            s::HEADER_ISSUER,
            &[("fo:text-align", "end"), ("fo:margin", "0cm")],
            &[("fo:font-size", "10pt")],
        )?;
        paragraph_style(writer, s::LIST_PARAGRAPH, &[("fo:margin", "0cm")], &[])?;
        paragraph_style(
            writer,
            s::TABLE_PARAGRAPH,
            &[("fo:margin", "0cm"), ("fo:text-align", "left")],
            &[("fo:font-size", "10pt")],
        )?;

        write_character_styles(writer)?;
        write_list_styles(writer)
    })
}

/// Déclare un style de paragraphe hérité de `Standard`, avec ses propriétés
/// de paragraphe et de caractère optionnelles.
fn paragraph_style<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    paragraph_props: &[(&str, &str)],
    text_props: &[(&str, &str)],
) -> Result<(), RenderError> {
    write_element(
        writer,
        "style:style",
        &[
            ("style:name", name),
            ("style:family", "paragraph"),
            ("style:parent-style-name", "Standard"),
        ],
        |writer| {
            if !paragraph_props.is_empty() {
                writer
                    .create_element("style:paragraph-properties")
                    .with_attributes(paragraph_props.iter().copied())
                    .write_empty()?;
            }
            if !text_props.is_empty() {
                writer
                    .create_element("style:text-properties")
                    .with_attributes(text_props.iter().copied())
                    .write_empty()?;
            }
            Ok(())
        },
    )
}

/// Déclare, pour chacune des 16 combinaisons de `bold`/`italic`/`underline`/
/// `strikeout` de [`content::Span`], le style de caractère correspondant.
/// Le nom généré doit rester en phase avec
/// [`crate::odt::content::span_style_name`].
fn write_character_styles<W: Write>(writer: &mut Writer<W>) -> Result<(), RenderError> {
    for combo in 0u8..16 {
        let bold = combo & 0b0001 != 0;
        let italic = combo & 0b0010 != 0;
        let underline = combo & 0b0100 != 0;
        let strikeout = combo & 0b1000 != 0;
        let name = format!(
            "Span_{}{}{}{}",
            bold as u8, italic as u8, underline as u8, strikeout as u8
        );

        write_element(
            writer,
            "style:style",
            &[("style:name", &name), ("style:family", "text")],
            |writer| {
                let mut props = writer.create_element("style:text-properties");
                if bold {
                    props = props.with_attribute(("fo:font-weight", "bold"));
                }
                if italic {
                    props = props.with_attribute(("fo:font-style", "italic"));
                }
                if underline {
                    props = props
                        .with_attribute(("style:text-underline-style", "solid"))
                        .with_attribute(("style:text-underline-width", "auto"))
                        .with_attribute(("style:text-underline-color", "font-color"));
                }
                if strikeout {
                    props = props
                        .with_attribute(("style:text-line-through-style", "solid"))
                        .with_attribute(("style:text-line-through-type", "single"));
                }
                props.write_empty()?;
                Ok(())
            },
        )?;
    }
    Ok(())
}

/// Styles de liste, un par valeur de [`content::ListMarker`], à un seul
/// niveau (les listes du corps d'un acte ne sont pas imbriquées).
fn write_list_styles<W: Write>(writer: &mut Writer<W>) -> Result<(), RenderError> {
    bullet_list_style(writer, s::LIST_DISC, "•")?;
    bullet_list_style(writer, s::LIST_CIRCLE, "◦")?;
    bullet_list_style(writer, s::LIST_SQUARE, "▪")?;
    numbered_list_style(writer, s::LIST_DECIMAL, "1")?;
    numbered_list_style(writer, s::LIST_LOWER_ALPHA, "a")?;
    numbered_list_style(writer, s::LIST_UPPER_ALPHA, "A")?;
    numbered_list_style(writer, s::LIST_LOWER_ROMAN, "i")?;
    numbered_list_style(writer, s::LIST_UPPER_ROMAN, "I")
}

fn bullet_list_style<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    bullet: &str,
) -> Result<(), RenderError> {
    write_element(
        writer,
        "text:list-style",
        &[("style:name", name)],
        |writer| {
            write_element(
                writer,
                "text:list-level-style-bullet",
                &[("text:level", "1"), ("text:bullet-char", bullet)],
                |writer| {
                    writer
                        .create_element("style:list-level-properties")
                        .with_attribute(("text:min-label-width", "0.6cm"))
                        .write_empty()?;
                    Ok(())
                },
            )
        },
    )
}

fn numbered_list_style<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    num_format: &str,
) -> Result<(), RenderError> {
    write_element(
        writer,
        "text:list-style",
        &[("style:name", name)],
        |writer| {
            write_element(
                writer,
                "text:list-level-style-number",
                &[
                    ("text:level", "1"),
                    ("style:num-format", num_format),
                    ("style:num-suffix", "."),
                ],
                |writer| {
                    writer
                        .create_element("style:list-level-properties")
                        .with_attribute(("text:min-label-width", "0.8cm"))
                        .write_empty()?;
                    Ok(())
                },
            )
        },
    )
}

/// Styles de tableau, partagés par toutes les tables du corps d'un acte
/// (l'éditeur ne modélise pas de mise en forme de tableau par table), et
/// mise en page de la page A4.
fn write_automatic_styles<W: Write>(writer: &mut Writer<W>) -> Result<(), RenderError> {
    write_element(writer, "office:automatic-styles", &[], |writer| {
        write_element(
            writer,
            "style:page-layout",
            &[("style:name", "Legal_PageLayout")],
            |writer| {
                writer
                    .create_element("style:page-layout-properties")
                    .with_attribute(("fo:page-width", "21.0cm"))
                    .with_attribute(("fo:page-height", "29.7cm"))
                    .with_attribute(("fo:margin-top", "2.5cm"))
                    .with_attribute(("fo:margin-bottom", "2.5cm"))
                    .with_attribute(("fo:margin-left", "2.5cm"))
                    .with_attribute(("fo:margin-right", "2cm"))
                    .write_empty()?;
                Ok(())
            },
        )?;
        write_element(
            writer,
            "style:style",
            &[("style:name", s::TABLE), ("style:family", "table")],
            |writer| {
                writer
                    .create_element("style:table-properties")
                    .with_attribute(("style:width", "17cm"))
                    .with_attribute(("table:align", "margins"))
                    .write_empty()?;
                Ok(())
            },
        )?;
        write_element(
            writer,
            "style:style",
            &[
                ("style:name", s::TABLE_COLUMN),
                ("style:family", "table-column"),
            ],
            |writer| {
                writer
                    .create_element("style:table-column-properties")
                    .with_attribute(("style:rel-width", "1*"))
                    .write_empty()?;
                Ok(())
            },
        )?;
        writer
            .create_element("style:style")
            .with_attribute(("style:name", s::TABLE_ROW))
            .with_attribute(("style:family", "table-row"))
            .write_empty()?;
        write_element(
            writer,
            "style:style",
            &[
                ("style:name", s::TABLE_CELL),
                ("style:family", "table-cell"),
            ],
            |writer| {
                writer
                    .create_element("style:table-cell-properties")
                    .with_attribute(("fo:border", "0.5pt solid #000000"))
                    .with_attribute(("fo:padding", "0.1cm"))
                    .write_empty()?;
                Ok(())
            },
        )?;
        write_element(
            writer,
            "style:style",
            &[("style:name", s::HEADER_TABLE), ("style:family", "table")],
            |writer| {
                writer
                    .create_element("style:table-properties")
                    .with_attribute(("style:width", "16.5cm"))
                    .with_attribute(("table:align", "margins"))
                    .write_empty()?;
                Ok(())
            },
        )?;
        write_element(
            writer,
            "style:style",
            &[
                ("style:name", s::HEADER_CELL),
                ("style:family", "table-cell"),
            ],
            |writer| {
                writer
                    .create_element("style:table-cell-properties")
                    .with_attribute(("fo:border", "none"))
                    .with_attribute(("fo:padding", "0cm"))
                    .write_empty()?;
                Ok(())
            },
        )
    })
}

/// Mise en page de base (A4, marges standard) partagée par tous les actes.
/// Si `authority_name` ou `issuer_name` est renseigné, la première page est
/// gouvernée par le master-page [`s::FIRST_PAGE_MASTER`] (en-tête
/// bloc-marque), qui bascule ensuite vers `Standard` (sans en-tête) via
/// `style:next-style-name`.
fn write_master_styles<W: Write>(
    writer: &mut Writer<W>,
    authority_name: Option<&str>,
    issuer_name: Option<&str>,
) -> Result<(), RenderError> {
    write_element(writer, "office:master-styles", &[], |writer| {
        if authority_name.is_some() || issuer_name.is_some() {
            write_element(
                writer,
                "style:master-page",
                &[
                    ("style:name", s::FIRST_PAGE_MASTER),
                    ("style:page-layout-name", "Legal_PageLayout"),
                    ("style:next-style-name", "Standard"),
                ],
                |writer| write_header(writer, authority_name, issuer_name),
            )?;
        }
        writer
            .create_element("style:master-page")
            .with_attribute(("style:name", "Standard"))
            .with_attribute(("style:page-layout-name", "Legal_PageLayout"))
            .write_empty()?;
        Ok(())
    })
}

/// En-tête de la première page : bloc-marque Marianne (« RÉPUBLIQUE
/// FRANÇAISE » + `authority_name`) à gauche, `issuer_name` à droite, alignés
/// sur une même ligne via un tableau à deux colonnes sans bordure.
fn write_header<W: Write>(
    writer: &mut Writer<W>,
    authority_name: Option<&str>,
    issuer_name: Option<&str>,
) -> Result<(), RenderError> {
    write_element(writer, "style:header", &[], |writer| {
        write_element(
            writer,
            "table:table",
            &[
                ("table:name", "HeaderBlocMarque"),
                ("table:style-name", s::HEADER_TABLE),
            ],
            |writer| {
                writer
                    .create_element("table:table-column")
                    .with_attribute(("table:style-name", s::TABLE_COLUMN))
                    .with_attribute(("table:number-columns-repeated", "2"))
                    .write_empty()?;
                write_element(writer, "table:table-row", &[], |writer| {
                    write_header_cell(writer, s::HEADER_BLOC_MARQUE, |writer| {
                        write_text_run(writer, "RÉPUBLIQUE FRANÇAISE")?;
                        if let Some(authority) = authority_name {
                            write_text_run(writer, "\n")?;
                            write_text_run(writer, authority)?;
                        }
                        Ok(())
                    })?;
                    write_header_cell(writer, s::HEADER_ISSUER, |writer| match issuer_name {
                        Some(issuer) => write_text_run(writer, issuer),
                        None => Ok(()),
                    })
                })
            },
        )
    })
}

/// Cellule d'en-tête borderless contenant un unique paragraphe `style`.
fn write_header_cell<W: Write>(
    writer: &mut Writer<W>,
    style: &str,
    text: impl FnOnce(&mut Writer<W>) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    write_element(
        writer,
        "table:table-cell",
        &[
            ("table:style-name", s::HEADER_CELL),
            ("office:value-type", "string"),
        ],
        |writer| write_element(writer, "text:p", &[("text:style-name", style)], text),
    )
}
