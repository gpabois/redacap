//! Génération de `META-INF/manifest.xml` : la liste des fichiers de
//! l'archive est fixe, elle ne dépend pas du contenu de l'acte.

use std::io::Write;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, Event};

use crate::error::RenderError;
use crate::odt::xml::write_element;

pub(crate) const MEDIA_TYPE: &str = "application/vnd.oasis.opendocument.text";

pub(crate) fn build() -> Result<Vec<u8>, RenderError> {
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    write_element(
        &mut writer,
        "manifest:manifest",
        &[
            (
                "xmlns:manifest",
                "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0",
            ),
            ("manifest:version", "1.3"),
        ],
        |writer| {
            file_entry(writer, "/", Some("1.3"), MEDIA_TYPE)?;
            file_entry(writer, "content.xml", None, "text/xml")?;
            file_entry(writer, "styles.xml", None, "text/xml")?;
            file_entry(writer, "meta.xml", None, "text/xml")
        },
    )?;

    Ok(writer.into_inner())
}

fn file_entry<W: Write>(
    writer: &mut Writer<W>,
    full_path: &str,
    version: Option<&str>,
    media_type: &str,
) -> Result<(), RenderError> {
    let mut attrs = vec![("manifest:full-path", full_path)];
    if let Some(version) = version {
        attrs.push(("manifest:version", version));
    }
    attrs.push(("manifest:media-type", media_type));

    writer
        .create_element("manifest:file-entry")
        .with_attributes(attrs)
        .write_empty()?;
    Ok(())
}
