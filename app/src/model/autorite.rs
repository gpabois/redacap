use std::ops::Deref;

pub struct Auhtority {
    pub label: String,
    pub kind: AuthorityKind
}

pub struct AuthorityId(String);

impl Deref for AuthorityId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub enum AuthorityKind {
    PréfetRégion,
    PréfetDépartement,
}