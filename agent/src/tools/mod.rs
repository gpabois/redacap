//! Implémentations concrètes des outils listés dans la documentation de
//! l'agent IA : chaque outil est un [`crate::Tool`] indépendant, à
//! enregistrer dans un [`crate::ToolRegistry`].

use std::sync::Arc;

use marie::{
    network::worker::{JobContext, server::WorkerServer},
    secret::SecretManager,
    tools::Toolable,
};

mod georisques;
mod legifrance;
mod secret;
// mod interaction;
// mod legifrance;
// mod legal_act_editor;

/// Enregistre les outils intégrés auprès de `worker`. `secret` est le
/// [`SecretManager`] partagé de l'application (voir `marie::secret`),
/// nécessaire pour déchiffrer les identifiants Légifrance/Géorisques
/// enregistrés via `/admin/integrations` (voir `agent::tools::secret`).
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