use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::error::StorageError;

/// Pool de connexions Postgres partagé par les repositories.
pub type Pool = PgPool;

/// Ouvre le pool de connexions vers la base applicative.
pub async fn connect(database_url: &str) -> Result<Pool, StorageError> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Construit un pool paresseux : `database_url` est validé syntaxiquement,
/// mais aucune connexion réseau n'est ouverte tant qu'aucune requête n'est
/// exécutée. Utilisé par les tests qui ont besoin d'une valeur [`Pool`]
/// sans base de données disponible.
pub fn connect_lazy(database_url: &str) -> Result<Pool, StorageError> {
    Ok(PgPoolOptions::new().connect_lazy(database_url)?)
}

/// Applique les migrations en attente de `storage/migrations`.
pub async fn migrate(pool: &Pool) -> Result<(), StorageError> {
    marie::persistency::postgres::run_migrations(pool).await?;
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Annule les migrations appliquées jusqu'à la version `target` (exclue).
///
/// `target = 0` annule l'ensemble des migrations. Nécessite un fichier `.down.sql`
/// pour chaque migration à annuler.
pub async fn revert(pool: &Pool, target: i64) -> Result<(), StorageError> {
    sqlx::migrate!("./migrations").undo(pool, target).await?;
    Ok(())
}
