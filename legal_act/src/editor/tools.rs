use crate::id::NodeId;

#[derive(Clone)]
pub enum Tool {
    Bold,
    Italic,
    Paragraph,
    AppendArticle(NodeId),
}

#[derive(Clone)]
pub struct ToolGroup {
    pub buttons: Vec<Tool>
}