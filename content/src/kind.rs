use strum_macros::{AsRefStr, EnumDiscriminants, EnumString};

/// Spécification d'un noeud de contenu, indépendante du backend de stockage
/// (mode direct ou Yrs).
#[derive(Hash, Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(AsRefStr, EnumString), name(ContentKind))]
pub enum NodeSpec {
    Root,
    Paragraph(Paragraph),
    Plain(String),
    Span(Span),
    List(List),
    ListItem(ListItem),
    Table(Table),
    Row(Row),
    Cell(Cell),
}

impl std::fmt::Display for ContentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl AsRef<str> for NodeSpec {
    fn as_ref(&self) -> &str {
        if let Self::Plain(text) = self {
            text.as_ref()
        } else {
            ""
        }
    }
}

impl NodeSpec {
    pub fn kind(&self) -> ContentKind {
        ContentKind::from(self)
    }

    /// Longueur en nombre de caractères. Toujours nulle pour un noeud non terminal.
    pub fn len(&self) -> usize {
        match self {
            Self::Plain(text) => text.chars().count(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ContentKind {
    pub fn new_default_node(self) -> NodeSpec {
        match self {
            ContentKind::Root => NodeSpec::Root,
            ContentKind::Paragraph => Paragraph.into(),
            ContentKind::Plain => NodeSpec::Plain(String::default()),
            ContentKind::Span => Span::default().into(),
            ContentKind::List => List::default().into(),
            ContentKind::ListItem => ListItem::default().into(),
            ContentKind::Table => Table.into(),
            ContentKind::Row => Row.into(),
            ContentKind::Cell => Cell.into(),
        }
    }

    #[inline]
    pub fn can_accept_child(self, child: Self) -> bool {
        self.allowed_children().contains(&child)
    }

    /// Calcule la chaîne de noeuds intermédiaires à insérer entre `self` et
    /// `to` pour que `self` devienne un descendant valide de `to`.
    ///
    /// Le résultat est ordonné du plus proche de `to` au plus proche de `self`
    /// (exclu) : `self` n'apparaît jamais dans le chemin retourné.
    pub fn correction_path(self, to: Self) -> Option<Vec<ContentKind>> {
        let mut path = self.find_ascending_path(to, vec![])?;
        path.pop();
        Some(path)
    }

    pub const TABLE: &[(Self, &[Self])] = &[
        (Self::Root, &[Self::Paragraph, Self::List, Self::Table]),
        (Self::Paragraph, &[Self::Plain, Self::Span]),
        (Self::Span, &[Self::Plain, Self::Span]),
        (Self::List, &[Self::ListItem]),
        (Self::ListItem, &[Self::Span, Self::Plain]),
        (Self::Table, &[Self::Row]),
        (Self::Row, &[Self::Cell]),
        (Self::Cell, &[Self::Span, Self::Plain]),
        (Self::Plain, &[]),
    ];

    pub fn find_ascending_path(self, to: Self, mut visited: Vec<ContentKind>) -> Option<Vec<ContentKind>> {
        visited.push(self);
        if self.parents().any(|par| par == to) {
            return Some(vec![self]);
        }

        let mut shortest = self
            .parents()
            .filter(|par| !visited.contains(par))
            .filter_map(|par| par.find_ascending_path(to, visited.clone()))
            .min_by_key(|path| path.len())?;

        shortest.push(self);
        Some(shortest)
    }

    #[inline]
    pub fn allowed_children(self) -> &'static [ContentKind] {
        Self::TABLE.iter().find(|(kind, _)| self == *kind).unwrap().1
    }

    /// Vrai si `self` et `other` acceptent exactement les mêmes genres
    /// d'enfants (ex: `Paragraph`, `ListItem` et `Cell` acceptent tous
    /// `Plain`/`Span`) : leur contenu est donc interchangeable, même si
    /// `self != other`.
    pub fn allowed_children_match(self, other: Self) -> bool {
        let ours = self.allowed_children();
        let theirs = other.allowed_children();
        ours.len() == theirs.len() && ours.iter().all(|kind| theirs.contains(kind))
    }

    pub fn parents(self) -> impl Iterator<Item = ContentKind> {
        Self::TABLE
            .iter()
            .filter(move |(_, children)| children.contains(&self))
            .map(|(parent, _)| *parent)
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

macro_rules! impl_from_node {
    ($variant:ident, $ty:ty) => {
        impl From<$ty> for NodeSpec {
            fn from(value: $ty) -> Self {
                NodeSpec::$variant(value)
            }
        }
    };
}

impl_from_node!(Paragraph, Paragraph);
impl_from_node!(Span, Span);
impl_from_node!(List, List);
impl_from_node!(ListItem, ListItem);
impl_from_node!(Table, Table);
impl_from_node!(Row, Row);
impl_from_node!(Cell, Cell);

#[derive(Hash, Debug, Clone, Default)]
pub struct Paragraph;

#[derive(Hash, Debug, Clone, Default)]
pub struct Span {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

#[derive(Hash, Debug, Clone, Default)]
pub struct List {
    pub marker: ListMarker,
    pub start: Option<u32>,
}

#[derive(Hash, Debug, Clone, Default)]
pub struct ListItem {
    pub marker: ListMarker,
}

#[derive(Hash, Debug, Clone, Default)]
pub struct Table;

#[derive(Hash, Debug, Clone, Default)]
pub struct Row;

#[derive(Hash, Debug, Clone, Default)]
pub struct Cell;

#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, Default, AsRefStr, EnumString)]
pub enum ListMarker {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

#[cfg(test)]
mod tests {
    use super::ContentKind::*;

    #[test]
    fn test_correction_path() {
        let got = Plain.correction_path(Root).unwrap();
        assert_eq!(got.as_slice(), &[Paragraph]);

        let got = Span.correction_path(Root).unwrap();
        assert_eq!(got.as_slice(), &[Paragraph]);

        let got = Paragraph.correction_path(Root).unwrap();
        assert_eq!(got.as_slice(), &[]);

        let got = Cell.correction_path(Root).unwrap();
        assert_eq!(got.as_slice(), &[Table, Row]);

        let got = Row.correction_path(Root).unwrap();
        assert_eq!(got.as_slice(), &[Table]);

        let got = Table.correction_path(Root).unwrap();
        assert_eq!(got.as_slice(), &[]);
    }
}
