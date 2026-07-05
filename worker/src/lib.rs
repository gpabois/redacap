//! Traitement asynchrone des tâches longues (rendition ODT/PDF, envoi de
//! courriels, consolidation des snapshots CRDT...).
//!
//! Seule la consolidation périodique des snapshots Yrs est implémentée pour
//! l'instant (voir [`legal_act`]) : les autres tâches longues restent à
//! faire.

mod legal_act;

use std::time::Duration;

/// Intervalle par défaut entre deux passes de consolidation des snapshots
/// Yrs, si `SNAPSHOT_CONSOLIDATION_INTERVAL_SECS` n'est pas défini.
const DEFAULT_SNAPSHOT_INTERVAL_SECS: u64 = 300;

/// Démarre le worker : connexion à la base, puis boucle de consolidation
/// périodique des snapshots Yrs jusqu'à un signal d'arrêt (Ctrl+C).
pub async fn run() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")?;
    let store = storage::connect(&database_url).await?;

    let interval_secs = std::env::var("SNAPSHOT_CONSOLIDATION_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_SNAPSHOT_INTERVAL_SECS);
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    println!("worker démarré, consolidation des snapshots toutes les {interval_secs}s");
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(error) = legal_act::consolidate_pending(&store).await {
                    eprintln!("échec de la consolidation périodique des snapshots : {error}");
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    println!("arrêt du worker");
    Ok(())
}
