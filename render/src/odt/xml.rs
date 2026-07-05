//! Utilitaires communs au-dessus de [`quick_xml::Writer`] pour la
//! génération des documents XML d'une archive ODF.

use std::io::Write;

use quick_xml::Writer;
use quick_xml::events::{BytesStart, BytesText, Event};

use crate::error::RenderError;

/// Écrit un élément dont le contenu peut échouer (rendition récursive du
/// corps de l'acte), en garantissant que la balise fermante correspond
/// toujours à la balise ouvrante : `end` est dérivée de `start` par
/// [`BytesStart::to_end`], jamais ressaisie.
pub(crate) fn write_element<W: Write>(
    writer: &mut Writer<W>,
    name: &str,
    attrs: &[(&str, &str)],
    inner: impl FnOnce(&mut Writer<W>) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    let start = BytesStart::new(name).with_attributes(attrs.iter().copied());
    let end = start.to_end().into_owned();
    writer.write_event(Event::Start(start))?;
    inner(writer)?;
    writer.write_event(Event::End(end))?;
    Ok(())
}

/// Écrit un texte brut comme contenu de `<office:text>`, en factorisant les
/// espaces successifs en `<text:s>`, les tabulations en `<text:tab/>` et les
/// retours à la ligne en `<text:line-break/>` (ODF §6.1.2/6.1.3). Le reste
/// de l'échappement (`&`, `<`, `>`, guillemets) est délégué à `quick_xml`.
pub(crate) fn write_text_run<W: Write>(
    writer: &mut Writer<W>,
    text: &str,
) -> Result<(), RenderError> {
    let mut plain = String::new();
    let mut spaces = 0u32;

    for ch in text.chars() {
        match ch {
            ' ' => spaces += 1,
            '\t' => {
                flush_spaces(writer, &mut plain, &mut spaces)?;
                flush_plain(writer, &mut plain)?;
                writer.create_element("text:tab").write_empty()?;
            }
            '\n' => {
                flush_spaces(writer, &mut plain, &mut spaces)?;
                flush_plain(writer, &mut plain)?;
                writer.create_element("text:line-break").write_empty()?;
            }
            c => {
                flush_spaces(writer, &mut plain, &mut spaces)?;
                plain.push(c);
            }
        }
    }
    flush_spaces(writer, &mut plain, &mut spaces)?;
    flush_plain(writer, &mut plain)
}

/// Écrit le nombre d'espaces accumulés dans `spaces` : le premier reste un
/// caractère espace littéral, les suivants sont encodés en `<text:s>` (voir
/// ODF §6.1.3).
fn flush_spaces<W: Write>(
    writer: &mut Writer<W>,
    plain: &mut String,
    spaces: &mut u32,
) -> Result<(), RenderError> {
    match *spaces {
        0 => {}
        1 => plain.push(' '),
        n => {
            plain.push(' ');
            flush_plain(writer, plain)?;
            writer
                .create_element("text:s")
                .with_attribute(("text:c", (n - 1).to_string().as_str()))
                .write_empty()?;
        }
    }
    *spaces = 0;
    Ok(())
}

/// Écrit le texte brut accumulé dans `plain` comme unique événement
/// `Event::Text`, échappé par `quick_xml`.
fn flush_plain<W: Write>(writer: &mut Writer<W>, plain: &mut String) -> Result<(), RenderError> {
    if !plain.is_empty() {
        writer.write_event(Event::Text(BytesText::new(plain)))?;
        plain.clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(text: &str) -> String {
        let mut writer = Writer::new(Vec::new());
        write_text_run(&mut writer, text).unwrap();
        String::from_utf8(writer.into_inner()).unwrap()
    }

    #[test]
    fn test_escapes_entities() {
        assert_eq!(render("A & B < C > D"), "A &amp; B &lt; C &gt; D");
    }

    #[test]
    fn test_collapses_repeated_spaces() {
        assert_eq!(render("a   b"), "a <text:s text:c=\"2\"/>b");
    }

    #[test]
    fn test_single_space_stays_literal() {
        assert_eq!(render("a b"), "a b");
    }

    #[test]
    fn test_tab_and_line_break() {
        assert_eq!(render("a\tb\nc"), "a<text:tab/>b<text:line-break/>c");
    }
}
