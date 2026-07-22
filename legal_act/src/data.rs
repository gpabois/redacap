use serde::{Deserialize, Serialize};
use shared::model::{User, UserId};
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
    user_id: UserId,
    user_name: String,
    span: Option<Span>,
    text: String
}

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct BodyRoot;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct CommentRoot;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Visa;

#[derive(Default, Debug, Hash, Clone, Serialize, Deserialize)]
pub struct VisaRoot;

#[derive(Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Considerant;

#[derive(Debug, Hash, Clone, Serialize, Deserialize)]
pub struct ConsiderantRoot;

#[derive(Debug, Hash, Clone, Serialize, Deserialize)]
pub struct Sur;

#[derive(Debug, Hash, Clone, Serialize, Deserialize)]
pub struct SurRoot;

/// Nœud de titre numéroté.
#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Titre {
    pub number: u32,
}

/// Nœud de section numérotée.
#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Section {
    pub number: u32,
}

/// Nœud de chapitre numéroté.
#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Chapitre {
    pub number: u32,
}

/// Nœud d'article numéroté.
#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Article {
    pub number: u32,
}

/// Nœud d'annexe numérotée.
#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Annexe {
    pub number: u32,
}

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct LibelleTitre(String);

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct LibelleSection(String);

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct LibelleChapitre(String);

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct LibelleArticle(String);

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct LibelleAnnexe(String);

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct ArticleBody;

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Paragraphe;

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Table;

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct TableRow;

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct TableCell;

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]

pub struct List {
    pub marker: ListMarker,
    pub start: Option<u32>,
}

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]

pub struct ListItem {
    pub marker: ListMarker,
}

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

#[derive(Default, Debug, Hash, Clone, Default, Serialize, Deserialize)]
pub struct Span {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

impl NodeData {
    /// Retourne le texte porté par le nœud, ou une chaîne vide si son type n'a pas de champ texte.
    pub(crate) fn text(&self) -> String {
        match self {
            NodeData::Comment(v) => v.text.clone(),
            NodeData::Visa(v) => v.0.clone(),
            NodeData::Considerant(v) => v.0.clone(),
            NodeData::Sur(v) => v.0.clone(),
            NodeData::LibelleTitre(v) => v.0.clone(),
            NodeData::LibelleSection(v) => v.0.clone(),
            NodeData::LibelleChapitre(v) => v.0.clone(),
            NodeData::LibelleArticle(v) => v.0.clone(),
            NodeData::LibelleAnnexe(v) => v.0.clone(),
            NodeData::Plain(v) => v.clone(),
            _ => String::default(),
        }
    }
}

impl From<NodeKind> for NodeData {
    fn from(value: NodeKind) -> Self {
        match value{
            NodeKind::Comment => Default::default().into(),
            NodeKind::CommentRoot => Default::default().into(),
            NodeKind::BodyRoot => Default::default().into(),
            NodeKind::Visa => Default::default().into(),
            NodeKind::Considerant => Default::default().into(),
            NodeKind::Sur => Default::default().into(),
            NodeKind::Titre => Default::default().into(),
            NodeKind::LibelleTitre => Default::default().into(),
            NodeKind::Section => Default::default().into(),
            NodeKind::LibelleSection => Default::default().into(),
            NodeKind::Chapitre => Default::default().into(),
            NodeKind::LibelleChapitre => Default::default().into(),
            NodeKind::Article => Default::default().into(),
            NodeKind::LibelleArticle => Default::default().into(),
            NodeKind::ArticleBody => Default::default().into(),
            NodeKind::Annexe => Default::default().into(),
            NodeKind::LibelleAnnexe => Default::default().into(),
            NodeKind::Paragraphe => Default::default().into(),
            NodeKind::Plain => Default::default().into(),
            NodeKind::Span => Default::default().into(),
            NodeKind::Table => Default::default().into(),
            NodeKind::TableRow => Default::default().into(),
            NodeKind::TableCell => Default::default().into(),
            NodeKind::List =>  Default::default().into(),
            NodeKind::ListItem => Default::default().into(),
            NodeKind::VisaRoot => Default::default().into(),
            NodeKind::ConsiderantRoot => Default::default().into(),
            NodeKind::SurRoot => Default::default().into(),
        }
    }
}

