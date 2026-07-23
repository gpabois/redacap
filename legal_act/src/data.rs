use loro::LoroMap;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumDiscriminants, EnumString};
use derive_more::From;

/// Spécification complète d'un nœud du corps d'un acte légal,
/// indépendante du backend de stockage (direct ou Yrs).
#[derive(Debug, Hash, Clone, EnumDiscriminants, Serialize, Deserialize, From)]
#[strum_discriminants(derive(AsRefStr, EnumString), name(NodeKind))]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "kind")]
#[serde(rename_all = "kebab-case")]
pub enum NodeData {
    Comment(Comment),
    CommentRoot(CommentRoot),
    BodyRoot(BodyRoot),
    VisaRoot(VisaRoot),
    ConsiderantRoot(ConsiderantRoot),
    SurRoot(SurRoot),
    Visa(Visa),
    Considerant(Considerant),
    Sur(Sur),
    Titre(Titre),
    LibelleTitre(LibelleTitre),
    Section(Section),
    LibelleSection(LibelleSection),
    Chapitre(Chapitre),
    LibelleChapitre(LibelleChapitre),
    Article(Article),
    LibelleArticle(LibelleArticle),
    ArticleBody(ArticleBody),
    Annexe(Annexe),
    LibelleAnnexe(LibelleAnnexe),
    Paragraphe(Paragraphe),
    Plain(String),
    Span(Span),
    Table(Table),
    TableRow(TableRow),
    TableCell(TableCell),
    List(List),
    ListItem(ListItem),
}

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub user_id: String,
    pub user_name: String,
    pub span: Option<Span>,
    pub text: String
}

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct BodyRoot;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct CommentRoot;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Visa;


#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct VisaRoot;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Considerant;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct ConsiderantRoot;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Sur;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct SurRoot;

/// Nœud de titre numéroté.
#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Titre {
    pub number: u32,
}

/// Nœud de section numérotée.
#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Section {
    pub number: u32,
}

/// Nœud de chapitre numéroté.
#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Chapitre {
    pub number: u32,
}

/// Nœud d'article numéroté.
#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Article {
    pub number: u32,
}

/// Nœud d'annexe numérotée.
#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Annexe {
    pub number: u32,
}

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct LibelleTitre;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct LibelleSection;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct LibelleChapitre;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct LibelleArticle;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct LibelleAnnexe;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct ArticleBody;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Paragraphe;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Table;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct TableRow;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct TableCell;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]

pub struct List {
    pub marker: ListMarker,
    pub start: u32,
}

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]

pub struct ListItem;

#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, Default, AsRefStr, EnumString, Serialize, Deserialize)]
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

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Span {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

impl From<Span> for LoroMap {
    fn from(value: Span) -> Self {
        let map = LoroMap::new();
        let _ = map.insert("bold", value.bold);
        let _ = map.insert("italic", value.italic);
        let _ = map.insert("underline", value.underline);
        let _ = map.insert("strikeout", value.strikeout);
        map
    }
}

impl NodeData {
    /// Retourne le texte porté par le nœud, ou une chaîne vide si son type n'a pas de champ texte.
    #[allow(dead_code)]
    pub(crate) fn text(&self) -> String {
        match self {
            NodeData::Plain(v) => v.clone(),
            _ => String::default(),
        }
    }
}

impl NodeKind {
    /// Indique si les nœuds de ce type portent du texte libre modifiable
    /// (voir [`NodeData::text`]).
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            NodeKind::Comment
                | NodeKind::Visa
                | NodeKind::Considerant
                | NodeKind::Sur
                | NodeKind::LibelleTitre
                | NodeKind::LibelleSection
                | NodeKind::LibelleChapitre
                | NodeKind::LibelleArticle
                | NodeKind::LibelleAnnexe
                | NodeKind::Plain
        )
    }
}

impl From<NodeKind> for NodeData {
    fn from(value: NodeKind) -> Self {
        match value{
            NodeKind::Comment => Comment::default().into(),
            NodeKind::CommentRoot => CommentRoot::default().into(),
            NodeKind::BodyRoot => BodyRoot::default().into(),
            NodeKind::Visa => Visa::default().into(),
            NodeKind::Considerant => Considerant::default().into(),
            NodeKind::Sur => Sur::default().into(),
            NodeKind::Titre => Titre::default().into(),
            NodeKind::LibelleTitre => LibelleTitre::default().into(),
            NodeKind::Section => Section::default().into(),
            NodeKind::LibelleSection => LibelleSection::default().into(),
            NodeKind::Chapitre => Chapitre::default().into(),
            NodeKind::LibelleChapitre => LibelleChapitre::default().into(),
            NodeKind::Article => Article::default().into(),
            NodeKind::LibelleArticle => LibelleArticle::default().into(),
            NodeKind::ArticleBody => ArticleBody::default().into(),
            NodeKind::Annexe => Annexe::default().into(),
            NodeKind::LibelleAnnexe => LibelleAnnexe::default().into(),
            NodeKind::Paragraphe => Paragraphe::default().into(),
            NodeKind::Plain => String::default().into(),
            NodeKind::Span => Span::default().into(),
            NodeKind::Table => Table::default().into(),
            NodeKind::TableRow => TableRow::default().into(),
            NodeKind::TableCell => TableCell::default().into(),
            NodeKind::List =>  List::default().into(),
            NodeKind::ListItem => ListItem::default().into(),
            NodeKind::VisaRoot => VisaRoot::default().into(),
            NodeKind::ConsiderantRoot => ConsiderantRoot::default().into(),
            NodeKind::SurRoot => SurRoot::default().into(),
        }
    }
}

