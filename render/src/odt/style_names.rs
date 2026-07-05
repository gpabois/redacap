//! Noms des styles ODF partagés entre `styles.xml` ([`super::styles`]) et
//! le corps du document ([`super::content`]), pour éviter toute divergence.

pub(crate) const VISA: &str = "Legal_Visa";
pub(crate) const CONSIDERANT: &str = "Legal_Considerant";
pub(crate) const SUR: &str = "Legal_Sur";
pub(crate) const TITRE: &str = "Legal_Titre";
pub(crate) const CHAPITRE: &str = "Legal_Chapitre";
pub(crate) const SECTION: &str = "Legal_Section";
pub(crate) const ARTICLE: &str = "Legal_Article";
pub(crate) const ANNEXE: &str = "Legal_Annexe";
pub(crate) const PARAGRAPHE: &str = "Legal_Paragraphe";
pub(crate) const LIST_PARAGRAPH: &str = "Legal_ListParagraph";
pub(crate) const TABLE_PARAGRAPH: &str = "Legal_TableParagraph";

pub(crate) const TABLE: &str = "Legal_Table";
pub(crate) const TABLE_COLUMN: &str = "Legal_Table_Column";
pub(crate) const TABLE_ROW: &str = "Legal_Table_Row";
pub(crate) const TABLE_CELL: &str = "Legal_Table_Cell";

pub(crate) const LIST_DISC: &str = "Legal_List_Disc";
pub(crate) const LIST_CIRCLE: &str = "Legal_List_Circle";
pub(crate) const LIST_SQUARE: &str = "Legal_List_Square";
pub(crate) const LIST_DECIMAL: &str = "Legal_List_Decimal";
pub(crate) const LIST_LOWER_ALPHA: &str = "Legal_List_LowerAlpha";
pub(crate) const LIST_UPPER_ALPHA: &str = "Legal_List_UpperAlpha";
pub(crate) const LIST_LOWER_ROMAN: &str = "Legal_List_LowerRoman";
pub(crate) const LIST_UPPER_ROMAN: &str = "Legal_List_UpperRoman";

/// Master-page utilisée pour la première page lorsqu'un en-tête
/// bloc-marque/issuer est renseigné (voir [`super::styles`]). Bascule vers
/// `Standard` dès la deuxième page via `style:next-style-name`.
pub(crate) const FIRST_PAGE_MASTER: &str = "First_Page";

pub(crate) const HEADER_TABLE: &str = "Legal_HeaderTable";
pub(crate) const HEADER_CELL: &str = "Legal_HeaderCell";
pub(crate) const HEADER_BLOC_MARQUE: &str = "Legal_HeaderBlocMarque";
pub(crate) const HEADER_ISSUER: &str = "Legal_HeaderIssuer";
