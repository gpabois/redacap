//! Génération de `meta.xml` : métadonnées minimales du document ODF.

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};

use crate::error::RenderError;
use crate::odt::xml::write_element;

pub(crate) fn build(title: &str) -> Result<Vec<u8>, RenderError> {
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    write_element(
        &mut writer,
        "office:document-meta",
        &[
            (
                "xmlns:office",
                "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
            ),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            (
                "xmlns:meta",
                "urn:oasis:names:tc:opendocument:xmlns:meta:1.0",
            ),
            ("office:version", "1.3"),
        ],
        |writer| {
            write_element(writer, "office:meta", &[], |writer| {
                writer
                    .create_element("dc:title")
                    .write_text_content(BytesText::new(title))?;
                writer
                    .create_element("meta:generator")
                    .write_text_content(BytesText::new("Redac'Ap"))?;
                Ok(())
            })
        },
    )?;

    Ok(writer.into_inner())
}
