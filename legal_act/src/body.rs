use crate::traits::node::{BodyRead, BodyWrite};
use crate::{BodyNodeId, NodeKind, NodeSpec};
use crate::{DirectBody, YrsBody};

/// Abstraction sur le backend de stockage du corps d'un acte légal.
/// Permet d'utiliser indifféremment le mode direct (mémoire locale) ou
/// le mode Yrs (CRDT collaboratif) dans les composants Leptos.
pub enum Body {
    Direct(DirectBody),
    Yrs(YrsBody),
}

impl From<DirectBody> for Body {
    fn from(b: DirectBody) -> Self {
        Body::Direct(b)
    }
}

impl From<YrsBody> for Body {
    fn from(b: YrsBody) -> Self {
        Body::Yrs(b)
    }
}

impl BodyRead for Body {
    fn root(&self) -> BodyNodeId {
        match self {
            Body::Direct(b) => b.root(),
            Body::Yrs(b) => b.root(),
        }
    }

    fn kind_of(&self, id: BodyNodeId) -> NodeKind {
        match self {
            Body::Direct(b) => b.kind_of(id),
            Body::Yrs(b) => b.kind_of(id),
        }
    }

    fn text_of(&self, id: BodyNodeId) -> String {
        match self {
            Body::Direct(b) => b.text_of(id),
            Body::Yrs(b) => b.text_of(id),
        }
    }

    fn parent_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        match self {
            Body::Direct(b) => b.parent_of(id),
            Body::Yrs(b) => b.parent_of(id),
        }
    }

    fn children_of(&self, id: BodyNodeId) -> Vec<BodyNodeId> {
        match self {
            Body::Direct(b) => b.children_of(id),
            Body::Yrs(b) => b.children_of(id),
        }
    }

    fn spec_of(&self, id: BodyNodeId) -> NodeSpec {
        match self {
            Body::Direct(b) => b.spec_of(id),
            Body::Yrs(b) => b.spec_of(id),
        }
    }

    fn title(&self) -> String {
        match self {
            Body::Direct(b) => b.title(),
            Body::Yrs(b) => b.title(),
        }
    }
}

impl BodyWrite for Body {
    fn create_node(&mut self, spec: NodeSpec) -> BodyNodeId {
        match self {
            Body::Direct(b) => b.create_node(spec),
            Body::Yrs(b) => b.create_node(spec),
        }
    }

    fn insert_child_at_unchecked(
        &mut self,
        parent: BodyNodeId,
        index: usize,
        child: BodyNodeId,
    ) -> anyhow::Result<()> {
        match self {
            Body::Direct(b) => b.insert_child_at_unchecked(parent, index, child),
            Body::Yrs(b) => b.insert_child_at_unchecked(parent, index, child),
        }
    }

    fn detach_unchecked(&mut self, id: BodyNodeId) -> anyhow::Result<()> {
        match self {
            Body::Direct(b) => b.detach_unchecked(id),
            Body::Yrs(b) => b.detach_unchecked(id),
        }
    }

    fn remove_subtree(&mut self, id: BodyNodeId) -> anyhow::Result<()> {
        match self {
            Body::Direct(b) => b.remove_subtree(id),
            Body::Yrs(b) => b.remove_subtree(id),
        }
    }

    fn insert_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, value: &str) {
        match self {
            Body::Direct(b) => b.insert_text_unchecked(id, char_index, value),
            Body::Yrs(b) => b.insert_text_unchecked(id, char_index, value),
        }
    }

    fn remove_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, char_count: usize) {
        match self {
            Body::Direct(b) => b.remove_text_unchecked(id, char_index, char_count),
            Body::Yrs(b) => b.remove_text_unchecked(id, char_index, char_count),
        }
    }

    fn set_spec_unchecked(&mut self, id: BodyNodeId, spec: NodeSpec) -> anyhow::Result<()> {
        match self {
            Body::Direct(b) => b.set_spec_unchecked(id, spec),
            Body::Yrs(b) => b.set_spec_unchecked(id, spec),
        }
    }

    fn set_title(&mut self, title: &str) {
        match self {
            Body::Direct(b) => b.set_title(title),
            Body::Yrs(b) => b.set_title(title),
        }
    }
}
