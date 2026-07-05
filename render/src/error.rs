use legal_act::NodeKind;

/// Erreurs pouvant survenir lors de la rendition d'un acte légal.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("erreur d'écriture XML : {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("erreur de génération de l'archive ODT : {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("erreur d'écriture dans l'archive ODT : {0}")]
    Io(#[from] std::io::Error),

    #[error("nœud {0} inattendu à cet emplacement du corps de l'acte")]
    UnexpectedNode(NodeKind),
}
