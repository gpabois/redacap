use std::str::FromStr;

use shared::id;

/// Identifiant d'un nœud du corps d'un acte légal.
///
/// Identique à [`content::ContentId`] dans sa structure : 128 bits,
/// générable localement sans coordination centrale, utilisable comme
/// clé dans un `yrs::Doc`.
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct BodyNodeId(id::ID);

impl BodyNodeId {
    pub fn new() -> Self {
        Self(id::generate_id())
    }

    pub(crate) fn from_raw(id: id::ID) -> Self {
        Self(id)
    }

    pub(crate) fn as_raw(self) -> id::ID {
        self.0
    }
}

impl Default for BodyNodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BodyNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for BodyNodeId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl<'a> TryFrom<&'a [u8]> for BodyNodeId {
    type Error = anyhow::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        id::ID::try_from(value).map(Self)
    }
}
