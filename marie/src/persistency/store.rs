use std::path::Path;
use std::sync::Arc;

use redb::{ReadableDatabase, ReadableTable, TableDefinition};

/// Objet persistable par un [`Store`] : associe son identifiant, son espace
/// de clés (`NAMESPACE`, pour cohabiter avec d'autres types d'objets dans le
/// même moteur de stockage) et son format d'encodage — indépendant du moteur
/// utilisé ([`RedbStore`] aujourd'hui, potentiellement un autre demain).
pub trait Persisted: Sized {
    type Id: std::fmt::Display + Send + Sync;

    const NAMESPACE: &'static str;

    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> anyhow::Result<Self>;
}

pub(crate) fn storage_key<T: Persisted>(id: &T::Id) -> Vec<u8> {
    format!("{}/{}", T::NAMESPACE, id).into_bytes()
}

/// Stockage générique pour tout objet du domaine implémentant [`Persisted`]
/// — chaque structure durable du cluster (le contenu CRDT des sessions
/// aujourd'hui, d'autres demain) s'appuie sur ce trait sans dépendre d'un
/// moteur de stockage particulier. [`RedbStore`] est l'implémentation par
/// défaut, mais un test ou un futur besoin peut en fournir une autre.
#[async_trait::async_trait]
pub trait Store<T: Persisted>: Send + Sync {
    async fn get(&self, id: &T::Id) -> anyhow::Result<Option<T>>;
    async fn put(&self, id: &T::Id, value: &T) -> anyhow::Result<()>;
    async fn delete(&self, id: &T::Id) -> anyhow::Result<()>;
    /// Tous les objets `T` actuellement stockés — utilisé pour la
    /// récupération à froid d'un catalogue entier (voir
    /// `model::catalog::store`), plutôt qu'un `get` par identifiant connu à
    /// l'avance.
    async fn list(&self) -> anyhow::Result<Vec<T>>;
}

const KV_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("kv");

/// [`Store`] adossé à [`redb`], un moteur embarqué pur Rust, fichier
/// unique, sans processus serveur à administrer — cohérent avec le reste de
/// `marie` (chaque nœud est autonome, découverte mDNS/LAN, pas de dépendance
/// à une infrastructure partagée). Tous les types [`Persisted`] partagent la
/// même table `redb`, distingués par leur `NAMESPACE`.
///
/// Les opérations `redb` sont synchrones (E/S fichier bloquantes) : chaque
/// appel est délégué à [`tokio::task::spawn_blocking`] pour ne pas bloquer
/// le runtime asynchrone.
pub struct RedbStore {
    db: Arc<redb::Database>,
}

impl RedbStore {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db = redb::Database::create(path)?;

        // Crée la table dès l'ouverture : évite d'avoir à distinguer "table
        // absente" de "clé absente" dans `get`.
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(KV_TABLE)?;
        }
        write_txn.commit()?;

        Ok(Self { db: Arc::new(db) })
    }
}

#[async_trait::async_trait]
impl<T: Persisted + Send + Sync> Store<T> for RedbStore {
    async fn get(&self, id: &T::Id) -> anyhow::Result<Option<T>> {
        let db = self.db.clone();
        let key = storage_key::<T>(id);

        let bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<u8>>> {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(KV_TABLE)?;
            Ok(table.get(key.as_slice())?.map(|value| value.value().to_vec()))
        })
        .await??;

        bytes.as_deref().map(T::decode).transpose()
    }

    async fn put(&self, id: &T::Id, value: &T) -> anyhow::Result<()> {
        let db = self.db.clone();
        let key = storage_key::<T>(id);
        let bytes = value.encode();

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(KV_TABLE)?;
                table.insert(key.as_slice(), bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await?
    }

    async fn delete(&self, id: &T::Id) -> anyhow::Result<()> {
        let db = self.db.clone();
        let key = storage_key::<T>(id);

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(KV_TABLE)?;
                table.remove(key.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await?
    }

    /// Parcourt toute la table et ne garde que les entrées dont la clé
    /// commence par `T::NAMESPACE` : `redb` ne propose pas d'espace de table
    /// par préfixe, et toutes les valeurs `Persisted` partagent la même table
    /// (voir [`KV_TABLE`]) — acceptable tant que le volume par type reste
    /// modeste (catalogues de configuration, pas des données volumineuses).
    async fn list(&self) -> anyhow::Result<Vec<T>> {
        let db = self.db.clone();
        let prefix = format!("{}/", T::NAMESPACE).into_bytes();

        let matches = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Vec<u8>>> {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(KV_TABLE)?;

            let mut matches = Vec::new();
            for entry in table.iter()? {
                let (key, value) = entry?;
                if key.value().starts_with(prefix.as_slice()) {
                    matches.push(value.value().to_vec());
                }
            }
            Ok(matches)
        })
        .await??;

        matches.iter().map(|bytes| T::decode(bytes)).collect()
    }
}
