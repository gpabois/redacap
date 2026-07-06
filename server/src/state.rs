use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use storage::Pool;

use crate::editor::state::EditorRooms;

/// État partagé de l'application, exposé aux handlers Axum du websocket.
pub struct AppState {
    pub store: Pool,
    pub rooms: EditorRooms,
    /// Clé de chiffrement/signature des cookies de session, dérivée de
    /// `SESSION_SECRET` (voir `server::auth::session`).
    pub session_key: Key,
    /// Clé de chiffrement/déchiffrement des secrets applicatifs (`client_secret`
    /// des fournisseurs OIDC, clé API des modèles IA, clés GéoRisques/Légifrance),
    /// dérivée de `SECRET_ENCRYPTION_KEY` (voir `shared::crypto`,
    /// `server::auth::crypto`). `None` si absente ou invalide : ces
    /// fonctionnalités sont alors indisponibles plutôt que de faire planter le
    /// serveur au démarrage.
    pub secret_encryption_key: Option<Vec<u8>>,
    /// URL publique de base de l'application (ex. `https://redacap.example.org`),
    /// nécessaire pour construire les `redirect_uri` OIDC. `None` si absente :
    /// l'authentification OIDC est alors indisponible.
    pub public_base_url: Option<String>,
    /// Client HTTP partagé pour les échanges OIDC (découverte, échange de code,
    /// userinfo), construit sans suivi de redirection (protection SSRF).
    pub oidc_http_client: openidconnect::reqwest::Client,
    /// État bootstrap (voir `Claude.md` § « Ajoute un état bootstrap... » et
    /// `server::guard::bootstrap_guard`) : `true` tant qu'aucun compte ne
    /// détient la permission globale `super_administrateur`, initialisé au
    /// démarrage depuis `storage::bootstrap::is_required`. Repasse à `false`
    /// dès la création du compte par `server::auth::bootstrap::create`,
    /// sans nouvelle requête en base pour les requêtes suivantes.
    pub bootstrap_required: Arc<AtomicBool>,
}

/// Enveloppe locale autour de `cookie::Key`, nécessaire pour satisfaire les
/// règles de cohérence (« orphan rules ») : ni `axum::extract::FromRef` ni
/// `axum_extra::extract::cookie::Key` ne sont définis dans ce crate, et
/// `Arc<AppState>` ne compte pas comme un type local aux yeux du compilateur
/// (seuls `&`, `&mut` et `Box` sont considérés « fondamentaux », pas `Arc`).
/// `PrivateCookieJar<SessionKey>` est donc utilisé à la place de
/// `PrivateCookieJar` (qui suppose implicitement `K = Key`).
#[derive(Clone)]
pub struct SessionKey(pub Key);

impl From<SessionKey> for Key {
    fn from(value: SessionKey) -> Self {
        value.0
    }
}

impl FromRef<Arc<AppState>> for SessionKey {
    fn from_ref(state: &Arc<AppState>) -> Self {
        SessionKey(state.session_key.clone())
    }
}
