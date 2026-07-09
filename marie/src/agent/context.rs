use std::ops::Deref;

use crate::agent::role::Role;

#[derive(Default)]
pub struct Context(Vec<ContextEntry>);

impl Deref for Context {
    type Target = [ContextEntry];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Context {
    pub fn push(&mut self, entry: ContextEntry) {
        self.0.push(entry)
    }

}

pub struct ContextEntry {
    pub role: Role,
    pub content: String
}
