/// Erreurs pouvant survenir lors d'un accès à la persistance.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("erreur de base de données : {0}")]
    Database(#[from] sqlx::Error),

    #[error("erreur de migration : {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("erreur de (dé)sérialisation : {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("identifiant invalide : {0}")]
    InvalidId(String),

    #[error("erreur d'identifiants (hachage du mot de passe) : {0}")]
    Credential(String),

    #[error("ressource introuvable")]
    NotFound,

    #[error("un super administrateur existe déjà : l'état bootstrap est terminé")]
    AlreadyBootstrapped,
}
