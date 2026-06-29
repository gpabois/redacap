use std::collections::HashMap;

use anyhow::bail;
use yrs::{Map, MapPrelim, ReadTxn};

use crate::{ContentId, crdt::{ContentArenaRef, node::{self, NodePrelim}}};

pub struct ContentRef {
    inner: yrs::MapRef,
    arena: ContentArenaRef,
    index: HashMap<ContentId, node::NodeRef>,
    kinds: HashMap<ContentId, crate::NodeKind>
}

pub struct ContentPrelim(yrs::MapPrelim);

impl ContentPrelim {
    pub fn new() -> Self {
        let root = ContentId::new();
        let node = NodePrelim::new(root, Root::default());
        
        let prelim = MapPrelim::from([
            ("arena", yrs::In::from(ArrayPrelim::from([node]))),
            ("root", yrs::In::from(root))
        ]);

        Self(prelim)
    }
}

impl From<ContentPrelim> for yrs::MapPrelim {
    fn from(value: ContentPrelim) -> Self {
        value.0
    }
}

impl From<ContentPrelim> for yrs::In {
    fn from(value: ContentPrelim) -> Self {
        let map_prelim = MapPrelim::from(value);
        yrs::In::from(map_prelim)
    }
}

impl<'a, Tx> TryFrom<(&'a Tx, yrs::MapRef)> for ContentRef where Tx: ReadTxn {
    type Error = anyhow::Error;

    fn try_from((txn, map): (&'a Tx, yrs::MapRef)) -> Result<Self, Self::Error> {
        let Some(yrs::Out::YArray(arena)) = map.get(txn, "arena") else { bail!("expecting arena field in MapRef of type ArrayRef") };


    }
}

impl TryFrom<yrs::Out> for ContentRef {
    type Error = anyhow::Error;

    fn try_from(value: yrs::Out) -> anyhow::Result<ContentRef> {
        use yrs::Out::YMap;
        let YMap(map) = value else { bail!("expecting yrs::MapRef from yrs::Out for ContentRef") };
        map.try_into()
    }
}