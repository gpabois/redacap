use crate::prelude::{NodeId, RefContent};

pub struct ContentLeafs<'a, Cx, Content: RefContent<Cx>> {
    content: &'a Content,
    cx: &'a Cx,
    current: Option<Content::NodeId>
}


impl<'a, Cx, Content: RefContent<Cx>> ContentLeafs<'a, Cx, Content> {
    pub fn new(cx: &'a Cx, content: &'a Content) -> Self {
        Self {
            content,
            cx,
            current: Some(content.root().first_leaf(cx, content))
        }
    }
}


impl<'a, Cx, Content: RefContent<Cx>> Iterator for ContentLeafs<'a, Cx, Content> {
    type Item = Content::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let curr = self.current?;
        self.current = curr.next_leaf(self.cx, self.content);
        Some(curr)
    }
}