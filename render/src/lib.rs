//! Rendition des actes légaux en documents finaux (ODT, à terme PDF).
//!
//! Les fonctions de ce crate sont pures : aucun accès disque ou réseau.
//! Les appels I/O (écriture du fichier, conversion PDF via LibreOffice...)
//! sont de la responsabilité du crate `server`.

mod error;
mod odt;

pub use error::RenderError;

use legal_act::LegalActRead;

/// Génère le document ODT correspondant à un acte légal (figé ou en
/// projet). Fonction pure.
pub fn render_odt<A: LegalActRead>(act: &A) -> Result<Vec<u8>, RenderError> {
    odt::build(act)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use legal_act::{
        Article, Body, NodeId, BodyAccess, Chapitre, DirectBody, LegalActKind,
        LegalActMeta, LegalActRead, NodeKind, NodeSpec, Titre,
    };

    use super::*;

    /// Acte légal minimal, uniquement pour les besoins des tests : un
    /// `LegalActRead` figé adossé à un `DirectBody`.
    struct FixtureAct {
        meta: LegalActMeta,
        title: String,
        body: Body,
    }

    impl LegalActRead for FixtureAct {
        type Body = Body;

        fn meta(&self) -> &LegalActMeta {
            &self.meta
        }

        fn title(&self) -> &str {
            &self.title
        }

        fn body(&self) -> &Body {
            &self.body
        }
    }

    fn fixture_with(build: impl FnOnce(&mut Body)) -> FixtureAct {
        let mut body: Body = DirectBody::new().into();
        build(&mut body);
        FixtureAct {
            meta: LegalActMeta::new(LegalActKind::ArretePrefectoral),
            title: "Arrêté préfectoral portant autorisation d'exploiter".to_string(),
            body,
        }
    }

    fn set_plain(body: &mut Body, id: NodeId, text: &str) {
        let leaf = body.first_leaf_of(id);
        body.insert_text(leaf, 0, text);
    }

    #[test]
    fn test_produces_a_valid_zip_with_mimetype_first_and_stored() {
        let act = fixture_with(|_| {});
        let bytes = render_odt(&act).unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(archive.len() >= 4);

        let mimetype = archive.by_index(0).unwrap();
        assert_eq!(mimetype.name(), "mimetype");
        assert_eq!(mimetype.compression(), zip::CompressionMethod::Stored);
    }

    #[test]
    fn test_content_xml_contains_titre_and_article_text() {
        let act = fixture_with(|body| {
            let titre = body
                .append_node(body.root(), NodeSpec::Titre(Titre::default()))
                .unwrap();
            let libelle = body
                .children_of(titre)
                .into_iter()
                .find(|&c| body.kind_of(c) == NodeKind::LibelleTitre)
                .unwrap();
            set_plain(body, libelle, "Dispositions générales");

            let article = body
                .append_node(titre, NodeSpec::Article(Article::default()))
                .unwrap();
            let article_libelle = body
                .children_of(article)
                .into_iter()
                .find(|&c| body.kind_of(c) == NodeKind::LibelleArticle)
                .unwrap();
            set_plain(body, article_libelle, "Objet de l'autorisation");
        });

        let bytes = render_odt(&act).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut content_xml = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("content.xml").unwrap(),
            &mut content_xml,
        )
        .unwrap();

        assert!(content_xml.contains("Titre I"));
        assert!(content_xml.contains("Dispositions générales"));
        assert!(content_xml.contains("Article 1"));
        assert!(
            content_xml.contains("Objet de l&apos;autorisation")
                || content_xml.contains("Objet de l'autorisation")
        );
    }

    #[test]
    fn test_nested_chapitre_and_article_are_rendered_in_order() {
        let act = fixture_with(|body| {
            let titre = body
                .append_node(body.root(), NodeSpec::Titre(Titre::default()))
                .unwrap();
            body.append_node(titre, NodeSpec::Chapitre(Chapitre::default()))
                .unwrap();
        });

        let bytes = render_odt(&act).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut content_xml = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("content.xml").unwrap(),
            &mut content_xml,
        )
        .unwrap();

        let titre_pos = content_xml.find("Titre I").unwrap();
        let chapitre_pos = content_xml.find("Chapitre 1").unwrap();
        assert!(
            titre_pos < chapitre_pos,
            "le chapitre doit apparaître après son titre parent"
        );
    }

    #[test]
    fn test_authority_and_issuer_produce_first_page_header() {
        let mut act = fixture_with(|body| {
            body.append_node(body.root(), NodeSpec::Visa).unwrap();
        });
        act.meta.authority_name = Some("DREAL".to_string());
        act.meta.issuer_name = Some("Le préfet de la région".to_string());

        let bytes = render_odt(&act).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();

        let mut styles_xml = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("styles.xml").unwrap(), &mut styles_xml)
            .unwrap();
        assert!(styles_xml.contains("RÉPUBLIQUE FRANÇAISE"));
        assert!(styles_xml.contains("DREAL"));
        assert!(styles_xml.contains("Le préfet de la région"));
        assert!(styles_xml.contains("style:name=\"First_Page\""));

        let mut content_xml = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("content.xml").unwrap(),
            &mut content_xml,
        )
        .unwrap();
        assert!(content_xml.contains("text:style-name=\"Legal_Visa_FirstPage\""));
    }

    #[test]
    fn test_no_authority_nor_issuer_omits_header() {
        let act = fixture_with(|body| {
            body.append_node(body.root(), NodeSpec::Visa).unwrap();
        });

        let bytes = render_odt(&act).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();

        let mut styles_xml = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("styles.xml").unwrap(), &mut styles_xml)
            .unwrap();
        assert!(!styles_xml.contains("style:name=\"First_Page\""));
        assert!(!styles_xml.contains("RÉPUBLIQUE FRANÇAISE"));
    }
}
