//! État partagé du serveur : le registre des salles de collaboration
//! (une par document édité) et le modèle de langage utilisé par la
//! boucle agentique.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use legal_act::{YrsBody, YrsReview};
use shared::id::ID;
use shared::model::{
    CreateLegalActReviewUpdate, CreateLegalActUpdate, LegalActReviewUpdate, LegalActUpdate,
};
use storage::Pool;
use tokio::sync::broadcast;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

/// Identité d'un utilisateur connecté à une salle, telle que suivie
/// côté serveur (voir [`super::protocol::PresenceUser`] pour son pendant sur
/// le fil, converti à la frontière websocket).
#[derive(Debug, Clone)]
pub struct Presence {
    pub user_id: ID,
    pub initial: String,
    pub color: String,
}

/// Entrée de présence d'un utilisateur dans une salle : son identité
/// affichée, et le nombre de connexions websocket ouvertes pour lui (un même
/// utilisateur peut ouvrir plusieurs onglets sur le même acte).
struct PresenceEntry {
    info: Presence,
    connections: usize,
}

/// Un document partagé entre les utilisateurs connectés à une même salle,
/// avec le canal de diffusion des mises à jour Yrs vers les pairs.
pub struct EditorRoom {
    pub body: tokio::sync::Mutex<YrsBody>,
    pub updates: broadcast::Sender<Vec<u8>>,
    /// Commentaires et notes de travail du projet (voir [`legal_act::Review`]),
    /// portés par un second document Yrs indépendant du corps (`body`) :
    /// journalisé dans ses propres tables (`legal_act_review_updates`/
    /// `legal_act_review_snapshots`, voir [`load_review`]), avec son propre
    /// canal de diffusion ([`Self::review_updates`]).
    pub review: tokio::sync::Mutex<YrsReview>,
    pub review_updates: broadcast::Sender<Vec<u8>>,
    /// Diffuse le snapshot complet des utilisateurs présents à chaque
    /// changement (voir [`EditorRooms::get_or_create`]/[`EditorRooms::release`]).
    pub presence: broadcast::Sender<Vec<Presence>>,
    /// Diffuse les messages de progression de l'agent (réflexion, appels
    /// d'outils, fin de tâche, interactions...) déjà sérialisés en JSON, à
    /// tous les pairs connectés à la salle (voir
    /// [`super::ports::WsUserInteraction`]) : contrairement à un canal propre
    /// à la connexion qui a démarré la tâche, ce canal de salle permet à une
    /// connexion qui rejoint après coup (nouvel onglet, reconnexion après un
    /// rechargement de page) de continuer à suivre une tâche déjà en cours
    /// plutôt que de perdre tout le fil de l'eau.
    pub agent_events: broadcast::Sender<String>,
    pool: Pool,
    /// Identifiant de l'acte légal édité, si `room_id` en est un (voir
    /// [`EditorRooms::get_or_create`]) : `None` pour les salons de
    /// démonstration, qui ne persistent alors ni ne journalisent rien.
    legal_act_id: Option<ID>,
    /// Prochain numéro de séquence à assigner dans `legal_act_updates` (voir
    /// [`Self::record_and_broadcast`]).
    next_seq: AtomicI64,
    /// Prochain numéro de séquence à assigner dans `legal_act_review_updates`
    /// (voir [`Self::record_and_broadcast_review`]).
    next_review_seq: AtomicI64,
    present_users: Mutex<HashMap<ID, PresenceEntry>>,
}

impl EditorRoom {
    pub(crate) fn new(
        pool: Pool,
        legal_act_id: Option<ID>,
        body: YrsBody,
        next_seq: i64,
        review: YrsReview,
        next_review_seq: i64,
    ) -> Arc<Self> {
        let (updates, _) = broadcast::channel(256);
        let (review_updates, _) = broadcast::channel(256);
        let (presence, _) = broadcast::channel(32);
        // Capacité plus large que les autres canaux : les fragments de
        // réflexion/contenu de l'agent sont diffusés au grain du modèle
        // (parfois un mot à la fois), un tour un peu long peut donc en
        // produire plusieurs centaines avant que la salle ne soit paisible ;
        // voir `server::editor::ws::reconcile_agent_lag` pour le filet de
        // sécurité côté serveur si cette capacité était malgré tout dépassée.
        let (agent_events, _) = broadcast::channel(2048);
        Arc::new(Self {
            body: tokio::sync::Mutex::new(body),
            updates,
            review: tokio::sync::Mutex::new(review),
            review_updates,
            presence,
            agent_events,
            pool,
            legal_act_id,
            next_seq: AtomicI64::new(next_seq),
            next_review_seq: AtomicI64::new(next_review_seq),
            present_users: Mutex::new(HashMap::new()),
        })
    }

    /// Identifiant de l'acte légal édité dans cette salle, `None` pour les
    /// salons de démonstration (voir [`Self::legal_act_id`] au champ) —
    /// utilisé par [`super::ws::spawn_agent_run`] pour résoudre le contexte
    /// (domaine, intentions, outils autorisés) du prompt système de l'agent.
    pub fn legal_act_id(&self) -> Option<ID> {
        self.legal_act_id
    }

    /// Diffuse `bytes` (mise à jour Yrs) à tous les pairs connectés à la
    /// salle, et la journalise en base au nom de `author_id` avec le
    /// prochain numéro de séquence — sans effet de persistance si `room_id`
    /// ne correspond à aucun acte légal existant (voir [`Self::legal_act_id`]).
    /// Utilisé aussi bien pour les mises à jour brutes reçues d'un client
    /// ([`super::ws::apply_and_broadcast`]) que pour les diffs produites par
    /// les outils de l'agent ([`super::ports::WsLegalActEditor`]).
    pub async fn record_and_broadcast(&self, author_id: &ID, bytes: Vec<u8>) {
        let _ = self.updates.send(bytes.clone());
        let Some(legal_act_id) = self.legal_act_id else {
            return;
        };
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let result = storage::legal_act::append_update(
            &self.pool,
            CreateLegalActUpdate {
                legal_act_id,
                seq,
                update: bytes,
                author_id: *author_id,
            },
        )
        .await;
        if let Err(error) = result {
            tracing::warn!("échec de la journalisation d'une mise à jour Yrs : {error}");
        }
    }

    /// Pendant de [`Self::record_and_broadcast`] pour le document de
    /// commentaires : diffuse `bytes` aux pairs et la journalise dans
    /// `legal_act_review_updates`.
    pub async fn record_and_broadcast_review(&self, author_id: &ID, bytes: Vec<u8>) {
        let _ = self.review_updates.send(bytes.clone());
        let Some(legal_act_id) = self.legal_act_id else {
            return;
        };
        let seq = self.next_review_seq.fetch_add(1, Ordering::SeqCst);
        let result = storage::legal_act_review::append_update(
            &self.pool,
            CreateLegalActReviewUpdate {
                legal_act_id,
                seq,
                update: bytes,
                author_id: *author_id,
            },
        )
        .await;
        if let Err(error) = result {
            tracing::warn!(
                "échec de la journalisation d'une mise à jour Yrs de commentaires : {error}"
            );
        }
    }

    fn present_snapshot(&self, users: &HashMap<ID, PresenceEntry>) -> Vec<Presence> {
        users.values().map(|entry| entry.info.clone()).collect()
    }

    /// Ajoute une connexion pour `user`, et renvoie le nouveau snapshot des
    /// utilisateurs présents (à diffuser sur [`Self::presence`]).
    fn join(&self, user: Presence) -> Vec<Presence> {
        let mut users = self.present_users.lock().expect("verrou non empoisonné");
        users
            .entry(user.user_id)
            .and_modify(|entry| entry.connections += 1)
            .or_insert(PresenceEntry {
                info: user,
                connections: 1,
            });
        self.present_snapshot(&users)
    }

    /// Retire une connexion de `user_id`. Renvoie le nouveau snapshot des
    /// utilisateurs présents, et `true` si plus aucune connexion (de
    /// quelque utilisateur que ce soit) n'est active sur la salle.
    fn leave(&self, user_id: &ID) -> (Vec<Presence>, bool) {
        let mut users = self.present_users.lock().expect("verrou non empoisonné");
        if let Some(entry) = users.get_mut(user_id) {
            entry.connections = entry.connections.saturating_sub(1);
            if entry.connections == 0 {
                users.remove(user_id);
            }
        }
        (self.present_snapshot(&users), users.is_empty())
    }
}

/// Registre des salles actives, une par identifiant de document (issu de
/// l'URL `/editor/{room_id}/ws`, qui correspond à l'identifiant du
/// `LegalAct` édité).
///
/// Une salle est créée à la première connexion, en rechargeant le dernier
/// état persisté (voir [`load_body`]) ; elle est retirée du registre dès que
/// sa dernière connexion se ferme (voir [`EditorRooms::release`]) — les
/// mises à jour étant déjà journalisées de manière incrémentale (voir
/// [`EditorRoom::record_and_broadcast`]), un utilisateur qui réaccède
/// ensuite au document reconstruit l'état le plus à jour depuis ce journal
/// plutôt que de repartir d'un document vierge.
#[derive(Default)]
pub struct EditorRooms(Mutex<HashMap<String, Arc<EditorRoom>>>);

impl EditorRooms {
    /// Rejoint la salle `room_id` au nom de `user`, en la créant si
    /// nécessaire. Chaque appel doit être équilibré par un appel à
    /// [`Self::release`] à la fermeture de la connexion. Renvoie la salle et
    /// le snapshot des utilisateurs présents après l'ajout de `user`.
    pub async fn get_or_create(
        &self,
        pool: &Pool,
        room_id: &str,
        user: Presence,
    ) -> (Arc<EditorRoom>, Vec<Presence>) {
        if let Some(joined) = self.join_existing(room_id, user.clone()) {
            return joined;
        }
        // Chargé hors du verrou (I/O) : une salle concurrente a pu être
        // créée entretemps, `HashMap::entry` ci-dessous s'assure alors de ne
        // conserver que la première, l'autre étant jetée sans conséquence —
        // hormis, dans le cas très rare de deux tout premiers rejoignants
        // simultanés d'un acte jamais encore édité, le risque que la
        // transaction de structure « genèse » journalisée par le perdant
        // (voir [`load_body`]) ne corresponde pas à la racine du document
        // effectivement conservé en mémoire (le gagnant de `or_insert_with`
        // ci-dessous). Un rechargement ultérieur reconstruirait alors une
        // racine différente ; sans conséquence tant qu'aucune édition
        // n'a eu lieu dans cette fenêtre de course.
        let legal_act_id = room_id.parse::<ID>().ok();
        let (body, next_seq) = load_body(pool, legal_act_id, &user.user_id).await;
        let (review, next_review_seq) = load_review(pool, legal_act_id, &user.user_id).await;
        let mut rooms = self.0.lock().expect("verrou non empoisonné");
        let room = rooms.entry(room_id.to_string()).or_insert_with(|| {
            EditorRoom::new(
                pool.clone(),
                legal_act_id,
                body,
                next_seq,
                review,
                next_review_seq,
            )
        });
        let snapshot = room.join(user);
        (room.clone(), snapshot)
    }

    fn join_existing(
        &self,
        room_id: &str,
        user: Presence,
    ) -> Option<(Arc<EditorRoom>, Vec<Presence>)> {
        let room = {
            let rooms = self.0.lock().expect("verrou non empoisonné");
            rooms.get(room_id)?.clone()
        };
        let snapshot = room.join(user);
        Some((room, snapshot))
    }

    /// Signale le départ d'une connexion de `user_id` sur `room_id`. Retire
    /// la salle du registre si c'était sa dernière connexion active (auquel
    /// cas il n'y a plus personne à qui diffuser un snapshot de présence :
    /// renvoie `None`) ; sinon renvoie le nouveau snapshot à diffuser aux
    /// pairs restants.
    pub fn release(&self, room_id: &str, user_id: &ID) -> Option<Vec<Presence>> {
        let room = {
            let rooms = self.0.lock().expect("verrou non empoisonné");
            rooms.get(room_id)?.clone()
        };
        let (snapshot, empty) = room.leave(user_id);
        if !empty {
            return Some(snapshot);
        }
        let mut rooms = self.0.lock().expect("verrou non empoisonné");
        if rooms
            .get(room_id)
            .is_some_and(|current| Arc::ptr_eq(current, &room))
        {
            rooms.remove(room_id);
        }
        None
    }
}

/// Reconstruit le [`YrsBody`] de `legal_act_id` à partir de son dernier
/// instantané persisté et des mises à jour postérieures, suivant
/// l'invariant de lecture documenté dans `storage::CLAUDE.md` (§ Actes
/// légaux — CRDT Yrs) : `snapshot + list_updates_since(snapshot.seq)`.
/// Renvoie aussi le prochain numéro de séquence à assigner.
///
/// Si l'acte n'a jamais encore été édité (aucun snapshot, aucune mise à
/// jour), initialise un document vierge et journalise immédiatement sa
/// transaction de structure interne (voir [`legal_act::YrsBody::init`])
/// comme mise à jour `seq = 1` au nom de `author_id` : cette transaction fixe
/// un identifiant de racine aléatoire qui, sans être journalisé, ne pourrait
/// pas être reconstruit à l'identique lors d'un futur rechargement.
///
/// Renvoie un document vierge (non journalisé) si `legal_act_id` est `None`
/// (salon de démonstration, sans acte légal associé).
async fn load_body(pool: &Pool, legal_act_id: Option<ID>, author_id: &ID) -> (YrsBody, i64) {
    let Some(legal_act_id) = legal_act_id else {
        return (YrsBody::new(), 1);
    };

    let snapshot = storage::legal_act::get_snapshot(pool, &legal_act_id)
        .await
        .ok()
        .flatten();
    let since_seq = snapshot.as_ref().map_or(0, |snapshot| snapshot.seq);
    let updates = storage::legal_act::list_updates_since(pool, &legal_act_id, since_seq)
        .await
        .unwrap_or_default();

    if snapshot.is_none() && updates.is_empty() {
        let body = YrsBody::new();
        let genesis = body
            .doc()
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let result = storage::legal_act::append_update(
            pool,
            CreateLegalActUpdate {
                legal_act_id,
                seq: 1,
                update: genesis,
                author_id: *author_id,
            },
        )
        .await;
        if let Err(error) = result {
            tracing::warn!("échec de la journalisation de la genèse de l'acte : {error}");
        }
        return (body, 2);
    }

    let next_seq = updates.last().map_or(since_seq, |update| update.seq) + 1;
    let doc = Doc::new();
    let body_map = doc.get_or_insert_map("body");
    if apply_snapshot_and_updates(
        &doc,
        snapshot.as_ref().map(|s| s.snapshot.as_slice()),
        &updates,
    ) {
        if let Ok(body) = YrsBody::open(doc, body_map) {
            return (body, next_seq);
        }
    }
    (YrsBody::new(), 1)
}

/// Applique successivement l'instantané `snapshot` (s'il existe) puis,
/// dans l'ordre, chaque mise à jour de `updates` au document `doc`
/// fraîchement créé. Renvoie `false` si l'instantané est corrompu (le
/// document reconstruit ne serait alors pas fiable) ; une mise à jour
/// individuellement corrompue est ignorée plutôt que de faire échouer toute
/// la reconstruction.
fn apply_snapshot_and_updates(
    doc: &Doc,
    snapshot: Option<&[u8]>,
    updates: &[LegalActUpdate],
) -> bool {
    if let Some(snapshot) = snapshot {
        let Ok(snapshot_update) = Update::decode_v1(snapshot) else {
            return false;
        };
        if doc.transact_mut().apply_update(snapshot_update).is_err() {
            return false;
        }
    }
    let mut txn = doc.transact_mut();
    for update in updates {
        if let Ok(update) = Update::decode_v1(&update.update) {
            let _ = txn.apply_update(update);
        }
    }
    true
}

/// Reconstruit le [`YrsReview`] (commentaires/notes de travail) de
/// `legal_act_id`, en tout point symétrique de [`load_body`] mais à partir
/// des tables dédiées `legal_act_review_updates`/`legal_act_review_snapshots`
/// (voir `storage::legal_act_review`) : les deux documents Yrs sont
/// persistés et reconstruits indépendamment.
async fn load_review(pool: &Pool, legal_act_id: Option<ID>, author_id: &ID) -> (YrsReview, i64) {
    let Some(legal_act_id) = legal_act_id else {
        return (YrsReview::new(), 1);
    };

    let snapshot = storage::legal_act_review::get_snapshot(pool, &legal_act_id)
        .await
        .ok()
        .flatten();
    let since_seq = snapshot.as_ref().map_or(0, |snapshot| snapshot.seq);
    let updates = storage::legal_act_review::list_updates_since(pool, &legal_act_id, since_seq)
        .await
        .unwrap_or_default();

    if snapshot.is_none() && updates.is_empty() {
        let review = YrsReview::new();
        let genesis = review
            .doc()
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let result = storage::legal_act_review::append_update(
            pool,
            CreateLegalActReviewUpdate {
                legal_act_id,
                seq: 1,
                update: genesis,
                author_id: *author_id,
            },
        )
        .await;
        if let Err(error) = result {
            tracing::warn!("échec de la journalisation de la genèse des commentaires : {error}");
        }
        return (review, 2);
    }

    let next_seq = updates.last().map_or(since_seq, |update| update.seq) + 1;
    let doc = Doc::new();
    let review_map = doc.get_or_insert_map("review");
    if apply_review_snapshot_and_updates(
        &doc,
        snapshot.as_ref().map(|s| s.snapshot.as_slice()),
        &updates,
    ) {
        if let Ok(review) = YrsReview::open(doc, review_map) {
            return (review, next_seq);
        }
    }
    (YrsReview::new(), 1)
}

/// Pendant de [`apply_snapshot_and_updates`] pour le document de commentaires.
fn apply_review_snapshot_and_updates(
    doc: &Doc,
    snapshot: Option<&[u8]>,
    updates: &[LegalActReviewUpdate],
) -> bool {
    if let Some(snapshot) = snapshot {
        let Ok(snapshot_update) = Update::decode_v1(snapshot) else {
            return false;
        };
        if doc.transact_mut().apply_update(snapshot_update).is_err() {
            return false;
        }
    }
    let mut txn = doc.transact_mut();
    for update in updates {
        if let Ok(update) = Update::decode_v1(&update.update) {
            let _ = txn.apply_update(update);
        }
    }
    true
}
