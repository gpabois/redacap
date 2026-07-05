//! Assemblage de l'archive ODT (fichier ZIP) à partir des différentes
//! parties XML générées : `mimetype`, `META-INF/manifest.xml`,
//! `content.xml`, `styles.xml`, `meta.xml`.

use std::io::{Cursor, Write as _};

use legal_act::LegalActRead;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, Event};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::RenderError;
use crate::odt::xml::write_element;
use crate::odt::{content, manifest, meta, styles};

pub(crate) fn build<A: LegalActRead>(act: &A) -> Result<Vec<u8>, RenderError> {
    let act_meta = act.meta();
    let authority_name = act_meta.authority_name.as_deref();
    let issuer_name = act_meta.issuer_name.as_deref();
    let has_first_page_header = authority_name.is_some() || issuer_name.is_some();

    let content_xml = build_content_xml(act, has_first_page_header)?;
    let styles_xml = styles::build(authority_name, issuer_name)?;
    let meta_xml = meta::build(act.title())?;
    let manifest_xml = manifest::build()?;

    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));

    // Le fichier `mimetype` doit être la première entrée de l'archive,
    // non compressée : c'est ce qui distingue un ODT d'un ZIP ordinaire.
    zip.start_file("mimetype", stored)?;
    zip.write_all(manifest::MEDIA_TYPE.as_bytes())?;

    zip.start_file("META-INF/manifest.xml", deflated)?;
    zip.write_all(&manifest_xml)?;

    zip.start_file("content.xml", deflated)?;
    zip.write_all(&content_xml)?;

    zip.start_file("styles.xml", deflated)?;
    zip.write_all(&styles_xml)?;

    zip.start_file("meta.xml", deflated)?;
    zip.write_all(&meta_xml)?;

    Ok(zip.finish()?.into_inner())
}

fn build_content_xml<A: LegalActRead>(
    act: &A,
    has_first_page_header: bool,
) -> Result<Vec<u8>, RenderError> {
    let mut writer = Writer::new(Vec::new());
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    write_element(
        &mut writer,
        "office:document-content",
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
            if has_first_page_header {
                content::write_first_page_styles(writer)?;
            } else {
                writer
                    .create_element("office:automatic-styles")
                    .write_empty()?;
            }
            write_element(writer, "office:body", &[], |writer| {
                write_element(writer, "office:text", &[], |writer| {
                    content::render_office_text(writer, act.body(), has_first_page_header)
                })
            })
        },
    )?;

    Ok(writer.into_inner())
}
