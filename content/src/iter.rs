use crate::prelude::{NodeId, ReadableContent};

pub struct ContentLeafs<'a, Content: ReadableContent> {
    content: &'a Content,
    current: Option<Content::NodeId>
}


impl<'a, Content: ReadableContent> ContentLeafs<'a, Content> {
    pub fn new(content: &'a Content) -> Self {
        Self {
            content,
            current: Some(content.root().first_leaf(content))
        }
    }
}


impl<'a, Content> Iterator for ContentLeafs<'a, Content> where Content: ReadableContent {
    type Item = Content::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let curr = self.current?;
        self.current = curr.next_leaf(self.content);
        Some(curr)
    }
}