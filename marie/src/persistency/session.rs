use yrs::StateVector;

use crate::{
    persistency::store::{Persisted, Store},
    session::{SessionId, crdt::YrsSession},
};

impl Persisted for YrsSession {
    type Id = SessionId;

    const NAMESPACE: &'static str = "session";

    /// Snapshot complet, encodé comme un diff depuis un vecteur d'état vide
    /// (voir [`YrsSession::diff_since`]) — c'est aussi le format que
    /// [`YrsSession::from_diff`] sait relire.
    fn encode(&self) -> Vec<u8> {
        self.diff_since(&StateVector::default())
    }

    fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Self::from_diff(bytes)
    }
}

/// Extension de [`Store<YrsSession>`] pour la synchronisation CRDT
/// incrémentale : diff depuis un vecteur d'état distant plutôt que l'objet
/// complet — utilisé par le nœud `Persistency` (voir `network::persistency`)
/// pour répondre à `RpcCall::FETCH_SESSION` sans transférer toute la session.
#[async_trait::async_trait]
pub trait SessionStore: Store<YrsSession> {
    /// Diff de la session depuis `state_vector`, ou `None` si elle est
    /// inconnue de ce nœud.
    async fn diff_since(&self, session_id: SessionId, state_vector: &StateVector) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(session) = self.get(&session_id).await? else {
            return Ok(None);
        };
        Ok(Some(session.diff_since(state_vector)))
    }
}

impl<S: Store<YrsSession>> SessionStore for S {}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use crate::id::IdGenerator;

    use super::*;
    use crate::persistency::store::storage_key;

    #[derive(Default)]
    struct MemoryStore(Mutex<HashMap<Vec<u8>, Vec<u8>>>);

    #[async_trait::async_trait]
    impl Store<YrsSession> for MemoryStore {
        async fn get(&self, id: &SessionId) -> anyhow::Result<Option<YrsSession>> {
            let key = storage_key::<YrsSession>(id);
            self.0.lock().unwrap().get(&key).map(|bytes| YrsSession::decode(bytes)).transpose()
        }

        async fn put(&self, id: &SessionId, value: &YrsSession) -> anyhow::Result<()> {
            let key = storage_key::<YrsSession>(id);
            self.0.lock().unwrap().insert(key, value.encode());
            Ok(())
        }

        async fn delete(&self, id: &SessionId) -> anyhow::Result<()> {
            let key = storage_key::<YrsSession>(id);
            self.0.lock().unwrap().remove(&key);
            Ok(())
        }

        async fn list(&self) -> anyhow::Result<Vec<YrsSession>> {
            let prefix = format!("{}/", YrsSession::NAMESPACE).into_bytes();
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, bytes)| YrsSession::decode(bytes))
                .collect()
        }
    }

    #[tokio::test]
    async fn test_unknown_session_returns_none() {
        let store = MemoryStore::default();
        let id = IdGenerator::default().next_id();

        assert!(store.get(&id).await.unwrap().is_none());
        assert!(store.diff_since(id, &StateVector::default()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_put_then_fetch_by_object_and_state_vector() {
        let store = MemoryStore::default();
        let id = IdGenerator::default().next_id();
        let session = YrsSession::new(id);

        store.put(&id, &session).await.unwrap();

        let reloaded = store.get(&id).await.unwrap().expect("session connue après put");
        assert_eq!(reloaded.id(), id);

        let diff = store.diff_since(id, &StateVector::default()).await.unwrap();
        assert!(diff.is_some());
    }
}
