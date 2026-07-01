use content::{List, ListItem, Span};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumString};

/// Spécification complète d'un nœud du corps d'un acte légal,
/// indépendante du backend de stockage (direct ou Yrs).
#[derive(Debug, Hash, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(AsRefStr, EnumString), name(NodeKind))]
pub enum NodeSpec {
    Root,
    Visa,
    Considerant,
    Sur,
    Titre(Titre),
    LibelleTitre,
    Section(Section),
    LibelleSection,
    Chapitre(Chapitre),
    LibelleChapitre,
    Article(Article),
    LibelleArticle,
    Annexe(Annexe),
    LibelleAnnexe,
    Paragraphe,
    Plain(String),
    Span(Span),
    Table,
    TableRow,
    TableCell,
    List(List),
    ListItem(ListItem),
}

/// Nœud de titre numéroté.
#[derive(Debug, Hash, Clone, Default)]
pub struct Titre {
    pub number: u32,
}

/// Nœud de section numérotée.
#[derive(Debug, Hash, Clone, Default)]
pub struct Section {
    pub number: u32,
}

/// Nœud de chapitre numéroté.
#[derive(Debug, Hash, Clone, Default)]
pub struct Chapitre {
    pub number: u32,
}

/// Nœud d'article numéroté.
#[derive(Debug, Hash, Clone, Default)]
pub struct Article {
    pub number: u32,
}

/// Nœud d'annexe numérotée.
#[derive(Debug, Hash, Clone, Default)]
pub struct Annexe {
    pub number: u32,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl NodeSpec {
    pub fn kind(&self) -> NodeKind {
        NodeKind::from(self)
    }

    /// Longueur en caractères (non nulle uniquement pour `Plain`).
    pub fn len(&self) -> usize {
        match self {
            Self::Plain(text) => text.chars().count(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Numéro local du nœud, `None` si le nœud n'est pas numéroté.
    pub fn number(&self) -> Option<u32> {
        match self {
            Self::Titre(t) => Some(t.number),
            Self::Section(s) => Some(s.number),
            Self::Chapitre(c) => Some(c.number),
            Self::Article(a) => Some(a.number),
            Self::Annexe(a) => Some(a.number),
            _ => None,
        }
    }

    /// Retourne une copie avec le numéro mis à jour, sans effet sur les
    /// nœuds non numérotés.
    #[must_use]
    pub fn with_number(mut self, n: u32) -> Self {
        match &mut self {
            Self::Titre(t) => t.number = n,
            Self::Section(s) => s.number = n,
            Self::Chapitre(c) => c.number = n,
            Self::Article(a) => a.number = n,
            Self::Annexe(a) => a.number = n,
            _ => {}
        }
        self
    }

    pub fn default_for(kind: NodeKind) -> Self {
        kind.default_spec()
    }
}

impl From<&str> for NodeSpec {
    fn from(value: &str) -> Self {
        Self::Plain(value.to_string())
    }
}

impl From<String> for NodeSpec {
    fn from(value: String) -> Self {
        Self::Plain(value)
    }
}

impl NodeKind {
    /// Spécification par défaut pour ce type de nœud.
    pub fn default_spec(self) -> NodeSpec {
        use NodeKind::*;
        match self {
            Root => NodeSpec::Root,
            Visa => NodeSpec::Visa,
            Considerant => NodeSpec::Considerant,
            Sur => NodeSpec::Sur,
            Titre => NodeSpec::Titre(crate::kind::Titre::default()),
            LibelleTitre => NodeSpec::LibelleTitre,
            Section => NodeSpec::Section(crate::kind::Section::default()),
            LibelleSection => NodeSpec::LibelleSection,
            Chapitre => NodeSpec::Chapitre(crate::kind::Chapitre::default()),
            LibelleChapitre => NodeSpec::LibelleChapitre,
            Article => NodeSpec::Article(crate::kind::Article::default()),
            LibelleArticle => NodeSpec::LibelleArticle,
            Annexe => NodeSpec::Annexe(crate::kind::Annexe::default()),
            LibelleAnnexe => NodeSpec::LibelleAnnexe,
            Paragraphe => NodeSpec::Paragraphe,
            Plain => NodeSpec::Plain(String::new()),
            Span => NodeSpec::Span(content::Span::default()),
            Table => NodeSpec::Table,
            TableRow => NodeSpec::TableRow,
            TableCell => NodeSpec::TableCell,
            List => NodeSpec::List(content::List::default()),
            ListItem => NodeSpec::ListItem(content::ListItem::default()),
        }
    }

    /// Vrai si ce type de nœud est un nœud de contenu (rich-text).
    /// Les fusions et divisions ne s'appliquent qu'aux nœuds de contenu.
    pub fn is_content_node(self) -> bool {
        use NodeKind::*;
        matches!(self, Paragraphe | Plain | Span | Table | TableRow | TableCell | List | ListItem)
    }

    /// Vrai si le nombre du nœud doit être maintenu automatiquement.
    pub fn is_numbered(self) -> bool {
        use NodeKind::*;
        matches!(self, Titre | Section | Chapitre | Article | Annexe)
    }

    /// Groupe d'ordre dans Root (plus petit = plus tôt).
    /// Renvoie `None` si ce type de nœud n'est pas un enfant direct du Root.
    pub fn root_order_group(self) -> Option<u8> {
        use NodeKind::*;
        match self {
            Visa => Some(0),
            Considerant => Some(1),
            Sur => Some(2),
            Titre | Section | Chapitre | Article => Some(3),
            Annexe => Some(4),
            _ => None,
        }
    }

    /// Vrai si `child` est un enfant direct autorisé sous `self`.
    pub fn can_accept_child(self, child: NodeKind) -> bool {
        self.allowed_children().contains(&child)
    }

    /// Types d'enfants directs autorisés sous ce type de nœud.
    pub fn allowed_children(self) -> &'static [NodeKind] {
        Self::CHILDREN_TABLE
            .iter()
            .find(|(kind, _)| *kind == self)
            .map(|(_, children)| *children)
            .unwrap_or(&[])
    }

    /// Table statique des enfants autorisés, structurée pour la
    /// vérification de cohérence à la construction.
    pub const CHILDREN_TABLE: &'static [(NodeKind, &'static [NodeKind])] = {
        use NodeKind::*;
        &[
            (Root, &[Visa, Considerant, Sur, Titre, Section, Chapitre, Article, Annexe]),
            (Visa, &[Plain, Span]),
            (Considerant, &[Plain, Span]),
            (Sur, &[Plain, Span]),
            (Titre, &[LibelleTitre, Chapitre, Section, Article]),
            (LibelleTitre, &[Plain, Span]),
            (Section, &[LibelleSection, Article]),
            (LibelleSection, &[Plain, Span]),
            (Chapitre, &[LibelleChapitre, Section, Article]),
            (LibelleChapitre, &[Plain, Span]),
            (Article, &[LibelleArticle, Paragraphe, Table, List]),
            (LibelleArticle, &[Plain, Span]),
            (Annexe, &[LibelleAnnexe, Article]),
            (LibelleAnnexe, &[Plain, Span]),
            (Paragraphe, &[Plain, Span]),
            (Span, &[Plain, Span]),
            (Table, &[TableRow]),
            (TableRow, &[TableCell]),
            (TableCell, &[Paragraphe, List]),
            (List, &[ListItem]),
            (ListItem, &[Plain, Span]),
            (Plain, &[]),
        ]
    };

    /// Libellé associé à un nœud structurel (`Titre` → `LibelleTitre`, etc.).
    /// Renvoie `None` pour les nœuds qui n'ont pas de libellé dédié.
    pub fn label_child_kind(self) -> Option<NodeKind> {
        use NodeKind::*;
        match self {
            Titre => Some(LibelleTitre),
            Section => Some(LibelleSection),
            Chapitre => Some(LibelleChapitre),
            Article => Some(LibelleArticle),
            Annexe => Some(LibelleAnnexe),
            _ => None,
        }
    }

    /// Vrai si ce nœud est un `Libellé*`.
    pub fn is_label(self) -> bool {
        use NodeKind::*;
        matches!(self, LibelleTitre | LibelleSection | LibelleChapitre | LibelleArticle | LibelleAnnexe)
    }
}

#[cfg(test)]
mod tests {
    use super::NodeKind::*;

    #[test]
    fn test_allowed_children() {
        assert!(Root.can_accept_child(Visa));
        assert!(Root.can_accept_child(Annexe));
        assert!(!Root.can_accept_child(Plain));
        assert!(!Root.can_accept_child(Paragraphe));

        assert!(Article.can_accept_child(Paragraphe));
        assert!(Article.can_accept_child(Table));
        assert!(Article.can_accept_child(List));
        assert!(Article.can_accept_child(LibelleArticle));
        assert!(!Article.can_accept_child(Section));

        assert!(Plain.allowed_children().is_empty());
    }

    #[test]
    fn test_root_order_group() {
        assert_eq!(Visa.root_order_group(), Some(0));
        assert_eq!(Considerant.root_order_group(), Some(1));
        assert_eq!(Sur.root_order_group(), Some(2));
        assert_eq!(Titre.root_order_group(), Some(3));
        assert_eq!(Article.root_order_group(), Some(3));
        assert_eq!(Annexe.root_order_group(), Some(4));
        assert_eq!(Plain.root_order_group(), None);
    }

    #[test]
    fn test_is_content_node() {
        assert!(Paragraphe.is_content_node());
        assert!(Plain.is_content_node());
        assert!(Span.is_content_node());
        assert!(Table.is_content_node());
        assert!(!Titre.is_content_node());
        assert!(!Article.is_content_node());
        assert!(!Root.is_content_node());
    }

    #[test]
    fn test_label_child_kind() {
        assert_eq!(Titre.label_child_kind(), Some(LibelleTitre));
        assert_eq!(Article.label_child_kind(), Some(LibelleArticle));
        assert_eq!(Annexe.label_child_kind(), Some(LibelleAnnexe));
        assert_eq!(Plain.label_child_kind(), None);
    }
}
