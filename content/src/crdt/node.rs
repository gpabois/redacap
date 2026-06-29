use std::{collections::HashMap, ops::Deref, str::FromStr};

use anyhow::bail;
use yrs::{Array, ArrayRef, Map, MapPrelim, ReadTxn};

use crate::{ContentId, NodeKind, NodeSpec};

#[derive(Clone)]
pub struct NodeRef {
    inner: yrs::MapRef,
    id: ContentId,
    kind: NodeKind
}

impl NodeRef {
    pub fn id(&self) -> ContentId {
        self.id
    }

    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    pub fn is_leaf<Txn>(&self, txn: &Txn) -> bool {
        self.children(txn).next().is_none()
    }

    pub fn children<Txn>(&self, txn: &Txn) -> impl Iterator<Item=ContentId> where Txn: ReadTxn {
        let array: ArrayRef = self.inner.get(txn, "children").unwrap().cast().unwrap();

        array.iter(txn).map(|out| {
            let node_id: ContentId = out.clone().cast().unwrap();
            node_id
        })
    }

    pub fn parent<Txn>(&self, txn: &Txn) -> Option<ContentId> {
        match self.inner.get(txn, "parent") {
            Some(yrs::Out::Any(yrs::Any::Null)) => None,
            Some(yrs::Out::Any(yrs::Any::Buffer(bytes))) => Some(ContentId::try_from(bytes.deref()).unwrap()),
            _ => panic!("expecting either null or bytes as value for parent field in NodeRef")
        }
    }

    pub fn next_sibling<Txn>(&self, txn: &Txn) -> Option<ContentId> {
        match self.inner.get(txn, "next_sibling") {
            Some(yrs::Out::Any(yrs::Any::Null)) => None,
            Some(yrs::Out::Any(yrs::Any::Buffer(bytes))) => Some(ContentId::try_from(bytes.deref()).unwrap()),
            _ => panic!("expecting either null or bytes as value for next_sibling field in NodeRef")
        }
    }

    pub fn prev_sibling<Txn>(&self, txn: &Txn) -> Option<ContentId> {
        match self.inner.get(txn, "prev_sibling") {
            Some(yrs::Out::Any(yrs::Any::Null)) => None,
            Some(yrs::Out::Any(yrs::Any::Buffer(bytes))) => Some(ContentId::try_from(bytes.deref()).unwrap()),
            _ => panic!("expecting either null or bytes as value for prev_sibling field in NodeRef")
        }
    }
}

impl<Txn> TryFrom<(&Txn, yrs::Out)> for NodeRef where Txn: ReadTxn {
    type Error = anyhow::Error;

    fn try_from((txn, value): (&Txn, yrs::Out)) -> Result<Self, Self::Error> {
        let yrs::Out::YMap(inner) = value else { bail!("expecting MapRef for ContentNodeRef")};
        let Some(yrs::Out::Any(yrs::Any::String(kind_str))) = inner.get(txn, "kind") else { bail!("expecting kind field for ContentNodeRef")};
        let Some(yrs::Out::Any(yrs::Any::Buffer(id_bytes))) = inner.get(txn, "id") else { bail!("expecting id field in MapRef for NodeRef") };
        let id = ContentId::try_from(id_bytes.deref())?;
        let kind = NodeKind::from_str(&kind_str)?;

        Ok(Self{inner, id, kind})
    }
}

pub struct NodePrelim {
    id: ContentId,
    parent: Option<ContentId>,
    next_sibling: Option<ContentId>,
    prev_sibling: Option<ContentId>,
    children: Vec<ContentId>,
    spec: NodeSpec
}

impl NodePrelim {
    pub fn new<S>(id: ContentId, spec: S) -> Self where NodeSpec: From<S> {
        Self {
            id,
            spec: spec.into(),
            next_sibling: None,
            prev_sibling: None,
            parent: None,
            children: vec![]
        }
    }
}

impl From<NodePrelim> for MapPrelim {
    fn from(value: NodePrelim) -> Self {
        let kind = NodeKind::from(&value.spec);

        let mut map  = HashMap::<&str, yrs::In>::new();

        map.insert("id", value.id.into());
        map.insert("kind", kind.into());
        map.insert("parent", value.parent.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map.insert("next_sibling", value.next_sibling.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map.insert("prev_sibling", value.prev_sibling.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map.insert("children", value.children.into_iter().map(yrs::In::from).collect::<yrs::ArrayPrelim>().into());
        map.insert("spec", value.spec.into());

        MapPrelim::from_iter(map.into_iter())
    }
}


impl From<NodePrelim> for yrs::In {
    fn from(value: NodePrelim) -> Self {
        let value = MapPrelim::from(value);
        yrs::In::Map(value)
    }
}
