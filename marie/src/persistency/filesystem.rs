use std::sync::Arc;

use futures::{StreamExt as _, TryStreamExt as _};
use object_store::{ObjectStore, ObjectStoreExt as _, aws::AmazonS3Builder, memory::InMemory, path::Path as ObjectPath};

use crate::session::SessionId;

/// Backend de stockage des fichiers de session (voir [`SessionFilesystem`]) :
/// un provider abstrait, choisi indépendamment du contenu qu'il stocke — la
/// mémoire pour les déploiements sans besoin de durabilité (tests, cluster
/// jetable), un bucket S3 ou compatible S3 (MinIO, etc.) pour le reste.
/// D'autres backends `object_store` (GCS, Azure, système de fichiers local)
/// peuvent s'ajouter ici sans changer [`SessionFilesystem`].
pub enum FilesystemConfig {
    /// Rien n'est conservé après l'arrêt du processus.
    Memory,
    S3 {
        bucket: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        /// `None` pour AWS S3 ; renseigné pour un provider compatible S3
        /// auto-hébergé (ex. `http://localhost:9000` pour MinIO).
        endpoint: Option<String>,
    },
}

impl FilesystemConfig {
    pub fn build(&self) -> anyhow::Result<Arc<dyn ObjectStore>> {
        match self {
            Self::Memory => Ok(Arc::new(InMemory::new())),
            Self::S3 { bucket, region, access_key_id, secret_access_key, endpoint } => {
                let mut builder = AmazonS3Builder::new()
                    .with_bucket_name(bucket)
                    .with_region(region)
                    .with_access_key_id(access_key_id)
                    .with_secret_access_key(secret_access_key);

                // Un provider compatible S3 auto-hébergé n'est en général pas
                // adressable par sous-domaine de bucket (style hébergé
                // virtuellement, le défaut AWS) : MinIO et consorts exigent le
                // style par chemin (`endpoint/bucket/clé`).
                if let Some(endpoint) = endpoint {
                    builder = builder
                        .with_endpoint(endpoint)
                        .with_virtual_hosted_style_request(false)
                        .with_allow_http(endpoint.starts_with("http://"));
                }

                Ok(Arc::new(builder.build()?))
            }
        }
    }
}

/// Fichiers d'une session, adossés à un [`ObjectStore`] abstrait (voir
/// [`FilesystemConfig`]) : toutes les sessions d'un même cluster partagent le
/// même backend, isolées les unes des autres par un préfixe de clé
/// (`{session_id}/...`).
#[derive(Clone)]
pub struct SessionFilesystem {
    store: Arc<dyn ObjectStore>,
}

impl SessionFilesystem {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    fn object_path(session_id: SessionId, path: &str) -> ObjectPath {
        ObjectPath::from(format!("{session_id}/{}", path.trim_start_matches('/')))
    }

    /// Contenu du fichier `path` de la session, ou `None` s'il n'existe pas.
    pub async fn read(&self, session_id: SessionId, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match self.store.get(&Self::object_path(session_id, path)).await {
            Ok(result) => Ok(Some(result.bytes().await?.to_vec())),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Écrit (ou remplace intégralement) le fichier `path` de la session.
    pub async fn write(&self, session_id: SessionId, path: &str, data: Vec<u8>) -> anyhow::Result<()> {
        self.store.put(&Self::object_path(session_id, path), data.into()).await?;
        Ok(())
    }

    /// Supprime le fichier `path` de la session. Ne fait rien s'il n'existe pas.
    pub async fn delete(&self, session_id: SessionId, path: &str) -> anyhow::Result<()> {
        match self.store.delete(&Self::object_path(session_id, path)).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Chemins de tous les fichiers connus de la session, relatifs à celle-ci.
    pub async fn list(&self, session_id: SessionId) -> anyhow::Result<Vec<String>> {
        let prefix = format!("{session_id}/");
        let entries = self.store.list(Some(&ObjectPath::from(session_id.to_string()))).try_collect::<Vec<_>>().await?;
        Ok(entries.into_iter().map(|meta| meta.location.to_string().trim_start_matches(&prefix).to_string()).collect())
    }

    /// Supprime tous les fichiers de la session — à appeler quand la session
    /// elle-même est supprimée (voir `RpcCall::DELETE_SESSION`).
    pub async fn delete_session(&self, session_id: SessionId) -> anyhow::Result<()> {
        let prefix = ObjectPath::from(session_id.to_string());
        let locations = self.store.list(Some(&prefix)).map_ok(|meta| meta.location).boxed();
        self.store.delete_stream(locations).try_collect::<Vec<_>>().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::id::IdGenerator;

    use super::*;

    fn filesystem() -> SessionFilesystem {
        SessionFilesystem::new(FilesystemConfig::Memory.build().unwrap())
    }

    #[tokio::test]
    async fn test_unknown_file_returns_none() {
        let fs = filesystem();
        let session_id = IdGenerator::default().next_id();

        assert!(fs.read(session_id, "notes.txt").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_write_then_read() {
        let fs = filesystem();
        let session_id = IdGenerator::default().next_id();

        fs.write(session_id, "notes.txt", b"bonjour".to_vec()).await.unwrap();

        let content = fs.read(session_id, "notes.txt").await.unwrap().expect("fichier connu après write");
        assert_eq!(content, b"bonjour");
    }

    #[tokio::test]
    async fn test_list_is_scoped_to_session() {
        let fs = filesystem();
        let session_a = IdGenerator::default().next_id();
        let session_b = IdGenerator::default().next_id();

        fs.write(session_a, "a.txt", b"a".to_vec()).await.unwrap();
        fs.write(session_a, "dir/b.txt", b"b".to_vec()).await.unwrap();
        fs.write(session_b, "c.txt", b"c".to_vec()).await.unwrap();

        let mut files = fs.list(session_a).await.unwrap();
        files.sort();
        assert_eq!(files, vec!["a.txt".to_string(), "dir/b.txt".to_string()]);
    }

    #[tokio::test]
    async fn test_delete_removes_single_file() {
        let fs = filesystem();
        let session_id = IdGenerator::default().next_id();

        fs.write(session_id, "a.txt", b"a".to_vec()).await.unwrap();
        fs.write(session_id, "b.txt", b"b".to_vec()).await.unwrap();
        fs.delete(session_id, "a.txt").await.unwrap();

        assert!(fs.read(session_id, "a.txt").await.unwrap().is_none());
        assert!(fs.read(session_id, "b.txt").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_delete_session_removes_all_files_but_other_sessions() {
        let fs = filesystem();
        let session_a = IdGenerator::default().next_id();
        let session_b = IdGenerator::default().next_id();

        fs.write(session_a, "a.txt", b"a".to_vec()).await.unwrap();
        fs.write(session_a, "dir/b.txt", b"b".to_vec()).await.unwrap();
        fs.write(session_b, "c.txt", b"c".to_vec()).await.unwrap();

        fs.delete_session(session_a).await.unwrap();

        assert!(fs.list(session_a).await.unwrap().is_empty());
        assert_eq!(fs.list(session_b).await.unwrap(), vec!["c.txt".to_string()]);
    }
}
