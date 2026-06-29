use std::str::FromStr;

use shared::id;

/// Identifiant d'un noeud de contenu.
///
/// Le même type d'identifiant est utilisé en mode direct et en mode Yrs : il
/// est suffisamment grand (128 bits) pour être généré localement par chaque
/// pair sans coordination centrale, ce qui le rend valide comme clé partagée
/// dans un `yrs::Doc`.
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(id::ID);

impl ContentId {
    pub fn new() -> Self {
        Self(id::generate_id())
    }

    pub(crate) fn from_raw(id: id::ID) -> Self {
        Self(id)
    }
}

impl Default for ContentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ContentId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl<'a> TryFrom<&'a [u8]> for ContentId {
    type Error = anyhow::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        id::ID::try_from(value).map(Self)
    }
}
