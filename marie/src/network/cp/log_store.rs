//! Stockage du log Raft (write-ahead log du cluster) + du vote courant.
//!
//! Implémentation en mémoire (`BTreeMap`) pour rester lisible : chaque
//! insertion/lecture pointe exactement vers l'endroit où brancher un vrai
//! backend persistant (sled, redb, rocksdb). En production, `append()` doit
//! flusher sur disque avant d'appeler le callback — sinon un crash juste
//! après un ACK à openraft peut faire perdre des entrées pourtant "commit".

use std::collections::BTreeMap;
use std::ops::RangeBounds;
use std::sync::Arc;

use openraft::storage::LogFlushed;
use openraft::storage::LogState;
use openraft::storage::RaftLogReader;
use openraft::storage::RaftLogStorage;
use openraft::Entry;
use openraft::LogId;
use openraft::OptionalSend;
use openraft::StorageError;
use openraft::Vote;
use tokio::sync::RwLock;

use super::types::{RaftNodeId, TypeConfig};

#[derive(Default)]
struct LogStoreInner {
    log: BTreeMap<u64, Entry<TypeConfig>>,
    vote: Option<Vote<RaftNodeId>>,
    last_purged: Option<LogId<RaftNodeId>>,
}

#[derive(Clone)]
pub struct LogStore {
    inner: Arc<RwLock<LogStoreInner>>,
    // TODO production: remplacer par un handle sled::Db / redb::Database
    // et faire persister `append`/`save_vote` avant de retourner.
}

impl LogStore {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(LogStoreInner::default())) }
    }
}

impl RaftLogReader<TypeConfig> for LogStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + std::fmt::Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<RaftNodeId>> {
        let inner = self.inner.read().await;
        Ok(inner.log.range(range).map(|(_, v)| v.clone()).collect())
    }
}

impl RaftLogStorage<TypeConfig> for LogStore {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<RaftNodeId>> {
        let inner = self.inner.read().await;
        let last = inner.log.values().last().map(|e| e.log_id);
        Ok(LogState {
            last_purged_log_id: inner.last_purged,
            last_log_id: last.or(inner.last_purged),
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn save_vote(&mut self, vote: &Vote<RaftNodeId>) -> Result<(), StorageError<RaftNodeId>> {
        let mut inner = self.inner.write().await;
        inner.vote = Some(*vote);
        // production: fsync ici avant de retourner Ok
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<RaftNodeId>>, StorageError<RaftNodeId>> {
        Ok(self.inner.read().await.vote)
    }

    /// Ajoute des entrées au log. `callback` DOIT être appelé une fois les
    /// entrées durablement persistées — c'est ce qui débloque la réplication
    /// côté openraft (voir `IOFlushed`).
    async fn append<I>(&mut self, entries: I, callback: LogFlushed<TypeConfig>) -> Result<(), StorageError<RaftNodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
    {
        {
            let mut inner = self.inner.write().await;
            for entry in entries {
                inner.log.insert(entry.log_id.index, entry);
            }
            // production: db.flush_async().await ici avant le callback
        }

        callback.log_io_completed(Ok(()));
        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<RaftNodeId>) -> Result<(), StorageError<RaftNodeId>> {
        let mut inner = self.inner.write().await;
        inner.log.split_off(&log_id.index);
        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<RaftNodeId>) -> Result<(), StorageError<RaftNodeId>> {
        let mut inner = self.inner.write().await;
        inner.log.retain(|idx, _| *idx > log_id.index);
        inner.last_purged = Some(log_id);
        Ok(())
    }
}
