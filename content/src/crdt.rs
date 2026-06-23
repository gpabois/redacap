use std::collections::HashMap;

use anyhow::{anyhow, bail};
use yrs::{Array, ArrayPrelim, ArrayRef, Map, MapPrelim, MapRef, block::{ItemPtr, Prelim}};

use crate::{ContentId, ContentNode, prelude::AsReadTxn};

pub struct ContentPrelim(yrs::MapPrelim);

impl ContentPrelim {
    pub fn new() -> Self {
        let prelim = MapPrelim::from([
            ("arena", ArrayPrelim::default()),
        ]);

        Self(prelim)
    }
}

impl Prelim for ContentPrelim {
    type Return = MapRef;

    fn into_content(self, txn: &mut yrs::TransactionMut) -> (yrs::block::ItemContent, Option<Self>) {
        let args = self.0.into_content(txn);
        (args.0, args.1.map(Self))
    }

    fn integrate(self, txn: &mut yrs::TransactionMut, inner_ref: yrs::branch::BranchPtr) {
        self.0.integrate(txn, inner_ref);
    }
}

pub struct ContentArenaRef(ArrayRef);

impl ContentArenaRef {
    pub fn iter<Cx: AsReadTxn>(&self, cx: &Cx) -> impl Iterator<Item=ContentNodeRef> {
        self.0.iter(cx.as_read_txn_ref()).flat_map(|map| map.try_into())
    }
}

impl TryFrom<yrs::Out> for ContentArenaRef {
    type Error = anyhow::Error;

    fn try_from(value: yrs::Out) -> Result<Self, Self::Error> {
        let yrs::Out::YArray(array_ref) = value else { bail!("expecting an ArrayRef for ContentArenaRef") };
        Ok(Self(array_ref))
    }
}

pub struct ContentRef {
    inner: MapRef,
    arena: ContentArenaRef,
    index: HashMap<ContentId, ContentNodeRef>
}

impl ContentRef {
    fn load<Cx>(cx: &Cx, inner: MapRef) -> anyhow::Result<Self> where Cx: AsReadTxn {
        let tx = cx.as_read_txn_ref();
        
        let arena: ContentArenaRef = inner.get(tx, "arena")
            .ok_or(anyhow!("expecting 'arena' attribute in MapRef for ContentRef"))?
            .try_into()?;

        let index = arena
            .iter(cx)
            .map(|content_node| (content_node.content_id(cx), content_node))
            .collect();

        Ok(ContentRef {
            inner,
            arena, 
            index
        })
    }
}

pub struct ContentNodeRef(MapRef);

impl ContentNodeRef {
    pub fn try_content_id<Cx>(&self, cx: &Cx) -> anyhow::Result<ContentId> where Cx: AsReadTxn {
        let id: String = self.0.get_as(cx.as_read_txn_ref(), "_id")?;
        let id: ContentId = id.parse()?;
        Ok(id)
    }

    pub fn content_id<Cx>(&self, cx: &Cx) -> ContentId where Cx: AsReadTxn {
        self.try_content_id(cx).unwrap()
    }
}

impl TryFrom<ItemPtr> for ContentNodeRef {
    type Error = anyhow::Error;

    fn try_from(value: ItemPtr) -> Result<Self, Self::Error> {
        let map_ref = MapRef::try_from(value).unwrap();
        Ok(Self(map_ref))
    }
}

impl TryFrom<yrs::Out> for ContentNodeRef {
    type Error = anyhow::Error;

    fn try_from(value: yrs::Out) -> Result<Self, Self::Error> {
        let yrs::Out::YMap(map_ref) = value else {bail!("expecting MapRef for ContentNodeRef")};
        Ok(Self(value))
    }
}

impl Prelim for ContentNode {
    type Return = ContentNodeRef;

    fn into_content(self, txn: &mut yrs::TransactionMut) -> (yrs::block::ItemContent, Option<Self>) {
        let kind = ContentNodeKind::from(&self.spec);

        MapPrelim::from([
            ("_id", self.id.to_string()),
            ("kind", kind),
            ("parent")
        ])
    }

    fn integrate(self, txn: &mut yrs::TransactionMut, inner_ref: yrs::branch::BranchPtr) {
        todo!()
    }
}