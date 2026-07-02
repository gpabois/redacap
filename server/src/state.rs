//! État partagé du serveur : le registre des salles de collaboration
//! (une par document édité) et le modèle de langage utilisé par la
//! boucle agentique.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent::LanguageModel;
use legal_act::YrsBody;
use tokio::sync::broadcast;

/// Un document partagé entre les utilisateurs connectés à une même salle,
/// avec le canal de diffusion des mises à jour Yrs vers les pairs.
pub struct Room {
    pub body: tokio::sync::Mutex<YrsBody>,
    pub updates: broadcast::Sender<Vec<u8>>,
}

impl Room {
    fn new() -> Arc<Self> {
        let (updates, _) = broadcast::channel(256);
        Arc::new(Self { body: tokio::sync::Mutex::new(YrsBody::new()), updates })
    }
}

/// Registre des salles actives, une par identifiant de document
/// (issu de l'URL `/ws/{room_id}`). Une salle est créée à la première
/// connexion et conservée en mémoire tant que le processus tourne.
#[derive(Default)]
pub struct Rooms(Mutex<HashMap<String, Arc<Room>>>);

impl Rooms {
    pub fn get_or_create(&self, room_id: &str) -> Arc<Room> {
        let mut rooms = self.0.lock().expect("verrou non empoisonné");
        rooms.entry(room_id.to_string()).or_insert_with(Room::new).clone()
    }
}

/// État partagé de l'application, exposé aux handlers Axum du websocket.
pub struct AppState {
    pub rooms: Rooms,
    /// `None` si aucun point de terminaison compatible n'est configuré
    /// (variables d'environnement `AGENT_BASE_URL`/`AGENT_API_KEY`/`AGENT_MODEL`
    /// absentes) : les tentatives de lancer la boucle agentique échouent alors
    /// proprement plutôt que de faire planter le serveur.
    pub model: Option<Arc<dyn LanguageModel>>,
}
