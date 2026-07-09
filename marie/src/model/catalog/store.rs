use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::ops::RangeBounds;
use std::sync::Arc;

use openraft::storage::{LogFlushed, RaftLogStorage, RaftStateMachine};
use openraft::{
    AnyError, LogId, LogState, OptionalSend, RaftLogReader, RaftSnapshotBuilder, Snapshot, SnapshotMeta,
    StorageError, StorageIOError, StoredMembership, Vote
};
use tokio::sync::RwLock;

use crate::model::catalog::types::{CatalogRequest, CatalogResponse, NodeId, RaftNode, TypeConfig};
use crate::model::declaration::{ModelDeclaration, ModelId};

type Entry = openraft::Entry<TypeConfig>;

#[derive(Debug, Default)]
struct LogStoreData {
    log: BTreeMap<u64, Entry>,
    vote: Option<Vote<NodeId>>,
    last_purged_log_id: Option<LogId<NodeId>>
}

/// Journal raft en mémoire, partagé entre le nœud raft et ses lecteurs de réplication.
#[derive(Debug, Clone, Default)]
pub struct LogStore {
    data: Arc<RwLock<LogStoreData>>
}

impl RaftLogReader<TypeConfig> for LogStore {
    async fn try_get_log_entries<RB>(&mut self, range: RB) -> Result<Vec<Entry>, StorageError<NodeId>>
    where RB: RangeBounds<u64> + Clone + std::fmt::Debug + OptionalSend {
        let data = self.data.read().await;
        Ok(data.log.range(range).map(|(_, entry)| entry.clone()).collect())
    }
}

impl RaftLogStorage<TypeConfig> for LogStore {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<NodeId>> {
        let data = self.data.read().await;
        let last_log_id = data.log.values().next_back().map(|entry| entry.log_id.clone()).or_else(|| data.last_purged_log_id.clone());

        Ok(LogState {
            last_purged_log_id: data.last_purged_log_id.clone(),
            last_log_id
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        self.data.write().await.vote = Some(vote.clone());
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        Ok(self.data.read().await.vote.clone())
    }

    async fn append<I>(&mut self, entries: I, callback: LogFlushed<TypeConfig>) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry> + OptionalSend,
        I::IntoIter: OptionalSend {
        let mut data = self.data.write().await;
        for entry in entries {
            data.log.insert(entry.log_id.index, entry);
        }
        drop(data);

        callback.log_io_completed(Ok(()));

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut data = self.data.write().await;
        data.log.split_off(&log_id.index);
        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut data = self.data.write().await;
        data.log = data.log.split_off(&(log_id.index + 1));
        data.last_purged_log_id = Some(log_id);
        Ok(())
    }
}

#[derive(Debug, Default)]
struct StateMachineData {
    last_applied_log: Option<LogId<NodeId>>,
    last_membership: StoredMembership<NodeId, RaftNode>,
    models: HashMap<ModelId, ModelDeclaration>
}

/// Machine à états du catalogue de modèles, répliquée par raft.
///
/// Les lectures (`get`/`list`) contournent le protocole raft et lisent directement l'état
/// local : elles ne sont donc pas linéarisables, seulement à jour du dernier `apply()` reçu
/// par ce nœud.
#[derive(Debug, Clone, Default)]
pub struct StateMachineStore {
    data: Arc<RwLock<StateMachineData>>
}

impl StateMachineStore {
    pub async fn get(&self, id: &str) -> Option<ModelDeclaration> {
        self.data.read().await.models.get(id).cloned()
    }

    pub async fn list(&self) -> HashMap<ModelId, ModelDeclaration> {
        self.data.read().await.models.clone()
    }
}

impl RaftSnapshotBuilder<TypeConfig> for StateMachineStore {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<NodeId>> {
        let data = self.data.read().await;

        let bytes = serde_json::to_vec(&data.models).map_err(|err| StorageIOError::write_snapshot(None, AnyError::new(&err)))?;

        let meta = SnapshotMeta {
            last_log_id: data.last_applied_log.clone(),
            last_membership: data.last_membership.clone(),
            snapshot_id: shared::id::generate_id().to_string()
        };

        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(bytes))
        })
    }
}

impl RaftStateMachine<TypeConfig> for StateMachineStore {
    type SnapshotBuilder = Self;

    async fn applied_state(&mut self) -> Result<(Option<LogId<NodeId>>, StoredMembership<NodeId, RaftNode>), StorageError<NodeId>> {
        let data = self.data.read().await;
        Ok((data.last_applied_log.clone(), data.last_membership.clone()))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<CatalogResponse>, StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry> + OptionalSend,
        I::IntoIter: OptionalSend {
        let mut data = self.data.write().await;
        let mut responses = Vec::new();

        for entry in entries {
            data.last_applied_log = Some(entry.log_id.clone());

            let response = match entry.payload {
                openraft::EntryPayload::Blank => CatalogResponse::default(),
                openraft::EntryPayload::Membership(membership) => {
                    data.last_membership = StoredMembership::new(Some(entry.log_id.clone()), membership);
                    CatalogResponse::default()
                }
                openraft::EntryPayload::Normal(CatalogRequest::Set { id, declaration }) => {
                    let previous = data.models.insert(id, declaration);
                    CatalogResponse { previous }
                }
                openraft::EntryPayload::Normal(CatalogRequest::Remove { id }) => {
                    let previous = data.models.remove(&id);
                    CatalogResponse { previous }
                }
            };

            responses.push(response);
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    async fn begin_receiving_snapshot(&mut self) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(&mut self, meta: &SnapshotMeta<NodeId, RaftNode>, snapshot: Box<Cursor<Vec<u8>>>) -> Result<(), StorageError<NodeId>> {
        let models = serde_json::from_slice(snapshot.get_ref()).map_err(|err| StorageIOError::read_snapshot(Some(meta.signature()), AnyError::new(&err)))?;

        let mut data = self.data.write().await;
        data.models = models;
        data.last_applied_log = meta.last_log_id.clone();
        data.last_membership = meta.last_membership.clone();

        Ok(())
    }

    async fn get_current_snapshot(&mut self) -> Result<Option<Snapshot<TypeConfig>>, StorageError<NodeId>> {
        // Les instantanés ne sont pas conservés entre deux appels : ils sont reconstruits à
        // la demande par `build_snapshot`.
        Ok(None)
    }
}
