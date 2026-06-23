#[derive(Clone)]
pub struct Article {
    pub id: String,
    pub kind: ArticleKind,
    pub label: String,
    pub numerotation: Vec<u32>
}

impl Article {
    pub fn new<S: ToString>(id: String, label: S, kind: ArticleKind) -> Self {
        let label = label.to_string();
        Self {
            id, kind, label, numerotation: Vec::default()
        }
    }
}

#[derive(Clone)]
pub enum ArticleKind {
    PlainArticle
}

