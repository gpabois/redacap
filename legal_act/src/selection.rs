use std::vec;

use crate::id::NodeId;
use crate::model::LegalActProject;

#[derive(Clone, Copy)]
pub struct Cursor {
    id: NodeId,
    pos: usize
}

#[derive(Clone, Copy)]
pub struct Span {
    start: Cursor,
    end: Cursor
}

impl Span {
    pub fn nodes(&self, act: LegalActProject) -> Vec<NodeId> {
        let Some(start_id) = act.first_leaf_of(&self.start.id) else { return vec![] };
        let Some(end_id) = act.last_leaf_of(&self.end.id) else { return vec![] };

        let leafs = act
            .leafs(&start_id)
            .take_while(|leaf| leaf != end_id);

        leafs.collect()
    }

}

pub enum CursorMode {
    Disabled,
    Cursor(Cursor),
    Span(Span)
}