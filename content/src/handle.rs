use crate::{ContentId, ContentKind, ContentRead, ContentWrite, NodeSpec, crdt::YrsContent, direct::DirectContent};

/// API opaque masquant le backend effectivement utilisé pour porter un
/// [`Content`](crate) : mode direct (en mémoire locale) ou mode Yrs (CRDT
/// collaboratif). Les composants Leptos manipulent uniquement ce type et
/// n'ont donc pas à savoir dans quel mode ils opèrent.
pub enum ContentHandle {
    Direct(DirectContent),
    Yrs(YrsContent),
}

impl ContentHandle {
    pub fn direct() -> Self {
        Self::Direct(DirectContent::new())
    }

    pub fn yrs() -> Self {
        Self::Yrs(YrsContent::new())
    }

    /// Construit un handle Yrs à partir d'un noeud `content` déjà
    /// initialisé (rejoint depuis un pair distant après synchronisation),
    /// que ce noeud soit la racine du `Doc` ou imbriqué dans une structure
    /// plus large (ex: le champ `content` d'un `LegalAct`).
    pub fn from_yrs_node(doc: yrs::Doc, content: yrs::MapRef) -> anyhow::Result<Self> {
        Ok(Self::Yrs(YrsContent::open(doc, content)?))
    }

    /// Construit un handle Yrs à partir d'un document autonome déjà
    /// synchronisé, dont le `Content` est la map racine `"content"`.
    pub fn from_yrs_doc(doc: yrs::Doc) -> anyhow::Result<Self> {
        let content = doc.get_or_insert_map("content");
        Self::from_yrs_node(doc, content)
    }
}

impl From<DirectContent> for ContentHandle {
    fn from(value: DirectContent) -> Self {
        Self::Direct(value)
    }
}

impl From<YrsContent> for ContentHandle {
    fn from(value: YrsContent) -> Self {
        Self::Yrs(value)
    }
}

impl ContentRead for ContentHandle {
    fn root(&self) -> ContentId {
        match self {
            Self::Direct(c) => c.root(),
            Self::Yrs(c) => c.root(),
        }
    }

    fn kind_of(&self, id: ContentId) -> ContentKind {
        match self {
            Self::Direct(c) => c.kind_of(id),
            Self::Yrs(c) => c.kind_of(id),
        }
    }

    fn text_of(&self, id: ContentId) -> String {
        match self {
            Self::Direct(c) => c.text_of(id),
            Self::Yrs(c) => c.text_of(id),
        }
    }

    fn parent_of(&self, id: ContentId) -> Option<ContentId> {
        match self {
            Self::Direct(c) => c.parent_of(id),
            Self::Yrs(c) => c.parent_of(id),
        }
    }

    fn children_of(&self, id: ContentId) -> Vec<ContentId> {
        match self {
            Self::Direct(c) => c.children_of(id),
            Self::Yrs(c) => c.children_of(id),
        }
    }

    fn spec_of(&self, id: ContentId) -> NodeSpec {
        match self {
            Self::Direct(c) => c.spec_of(id),
            Self::Yrs(c) => c.spec_of(id),
        }
    }
}

impl ContentWrite for ContentHandle {
    fn create_node<N>(&mut self, spec: N) -> ContentId
    where
        NodeSpec: From<N>,
    {
        match self {
            Self::Direct(c) => c.create_node(spec),
            Self::Yrs(c) => c.create_node(spec),
        }
    }

    fn insert_child_at(&mut self, parent: ContentId, index: usize, child: ContentId) -> anyhow::Result<()> {
        match self {
            Self::Direct(c) => c.insert_child_at(parent, index, child),
            Self::Yrs(c) => c.insert_child_at(parent, index, child),
        }
    }

    fn detach_unchecked(&mut self, id: ContentId) -> anyhow::Result<()> {
        match self {
            Self::Direct(c) => c.detach_unchecked(id),
            Self::Yrs(c) => c.detach_unchecked(id),
        }
    }

    fn remove_node(&mut self, id: ContentId) -> anyhow::Result<()> {
        match self {
            Self::Direct(c) => c.remove_node(id),
            Self::Yrs(c) => c.remove_node(id),
        }
    }

    fn insert_text(&mut self, id: ContentId, char_index: usize, value: &str) {
        match self {
            Self::Direct(c) => c.insert_text(id, char_index, value),
            Self::Yrs(c) => c.insert_text(id, char_index, value),
        }
    }

    fn remove_text(&mut self, id: ContentId, char_index: usize, char_count: usize) {
        match self {
            Self::Direct(c) => c.remove_text(id, char_index, char_count),
            Self::Yrs(c) => c.remove_text(id, char_index, char_count),
        }
    }

    fn set_spec(&mut self, id: ContentId, spec: NodeSpec) -> anyhow::Result<()> {
        match self {
            Self::Direct(c) => c.set_spec(id, spec),
            Self::Yrs(c) => c.set_spec(id, spec),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(mut body: ContentHandle) {
        let id = body.append_content(body.root(), "hello").unwrap();
        body.insert_text(id, 5, " world");
        assert_eq!(body.text_of(id), "hello world");
        assert_eq!(body.kind_of(body.parent_of(id).unwrap()), ContentKind::Paragraph);
    }

    #[test]
    fn test_both_backends_through_the_opaque_handle() {
        exercise(ContentHandle::direct());
        exercise(ContentHandle::yrs());
    }
}
