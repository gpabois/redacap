//! Implémentations concrètes des outils listés dans la documentation de
//! l'agent IA : chaque outil est un [`crate::Tool`] indépendant, à
//! enregistrer dans un [`crate::ToolRegistry`].

#[cfg(feature = "server")]
use std::sync::Arc;

#[cfg(feature = "server")]
use marie::{
    network::worker::{JobContext, server::WorkerServer},
    secret::SecretManager,
    tools::Toolable,
};

// `georisques`/`legifrance` dépendent de reqwest/tokio/async-trait (voir la
// feature `server` dans Cargo.toml), indisponibles côté client WASM ; les
// crates qui désactivent les features par défaut d'`agent` (ex. `legal_act`)
// ne doivent donc pas les compiler.
#[cfg(feature = "server")]
mod georisques;
#[cfg(feature = "server")]
mod legifrance;
#[cfg(feature = "server")]
mod secret;
// mod interaction;
// mod legifrance;
// mod legal_act_editor;

/// Enregistre les outils intégrés auprès de `worker`. `secret` est le
/// [`SecretManager`] partagé de l'application (voir `marie::secret`),
/// nécessaire pour déchiffrer les identifiants Légifrance/Géorisques
/// enregistrés via `/admin/integrations` (voir `agent::tools::secret`).
#[cfg(feature = "server")]
pub fn register_builtin_tools(
    pool: storage::Pool,
    secret: SecretManager,
    mut worker: WorkerServer<JobContext>,
) {
    {
        let pool = pool.clone();
        let secret = secret.clone();
        let factory: georisques::GeorisquesClientFactory =
            Arc::new(move || Box::pin(georisques::create_client(pool.clone(), secret.clone())));
        georisques::GetAiot(factory).register_executor(&mut worker);
    }

    {
        let pool = pool.clone();
        let secret = secret.clone();
        let factory: legifrance::LegifranceClientFactory =
            Arc::new(move || Box::pin(legifrance::create_client(pool.clone(), secret.clone())));
        legifrance::SearchLegifrance(factory).register_executor(&mut worker);
    }
}