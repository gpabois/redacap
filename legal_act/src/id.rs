use shared::id;

/// Identifiant d'un nœud du corps d'un acte légal.
///
/// Identique à [`content::ContentId`] dans sa structure : 128 bits,
/// générable localement sans coordination centrale, utilisable comme
/// clé dans un `yrs::Doc`.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct NodeId(pub(crate) String);

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        NodeId(value.to_string())
    }
}

impl NodeId {
    pub fn new() -> Self {
        Self(id::generate_id().to_string())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for NodeId {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}
