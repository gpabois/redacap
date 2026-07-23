use std::vec;

use loro::LoroMap;

use crate::id::NodeId;
use crate::model::LegalActProject;

#[derive(Clone)]
pub struct Cursor {
    pub id: NodeId,
    pub pos: usize
}

impl Cursor {
    pub fn into_loro_map(self) -> LoroMap {
        self.into()
    }

    pub fn within(&self, id: &NodeId) -> bool {
        self.id == *id
    }
}

impl From<Cursor> for LoroMap {
    fn from(value: Cursor) -> Self {
        let map = LoroMap::new();
        let _ = map.insert("id", value.id.to_string());
        let _ = map.insert("position", value.pos as u32);
        map
    }
}

#[derive(Clone)]
pub struct Span {
    pub start: Cursor,
    pub end: Cursor
}

impl From<Span> for LoroMap {
    fn from(value: Span) -> Self {
        let map = LoroMap::new();
        let _ = map.insert_container("start", value.start.into_loro_map());
        let _ = map.insert_container("end", value.end.into_loro_map());
        map
    }
}

impl Span {
    pub fn nodes(&self, act: LegalActProject) -> Vec<NodeId> {
        let Some(start_id) = act.first_leaf_of(&self.start.id) else { return vec![] };
        let Some(end_id) = act.last_leaf_of(&self.end.id) else { return vec![] };

        let leafs = act
            .leafs(&start_id)
            .take_while(|leaf| *leaf != end_id);

        leafs.collect()
    }

    /// Indique si `id` est une feuille entièrement couverte par la sélection,
    /// c'est-à-dire ni la borne de début ni celle de fin (dont seule une
    /// portion est sélectionnée).
    pub fn covers(&self, act: LegalActProject, id: &NodeId) -> bool {
        self.start.id != *id && self.end.id != *id && self.nodes(act).contains(id)
    }
}
