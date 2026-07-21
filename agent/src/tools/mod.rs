//! Implémentations concrètes des outils listés dans la documentation de
//! l'agent IA : chaque outil est un [`crate::Tool`] indépendant, à
//! enregistrer dans un [`crate::ToolRegistry`].

use marie::network::worker::{JobContext, server::WorkerServer};

mod georisques;
// mod interaction;
// mod legifrance;
// mod legal_act_editor;

pub fn register_builtin_tools(pool: storage::Pool, worker: WorkerServer<JobContext>) {
    {
        let pool = pool.clone();
        georisques::GetAiot(Arc::new(move || georisques::create_client(pool)));
    }

}