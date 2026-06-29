//! Outils `legifrance_search` et `legifrance_fetch`, qui interrogent l'API
//! officielle Légifrance sur le portail PISTE.
//!
//! L'authentification PISTE est de type OAuth2 `client_credentials` : le
//! jeton est obtenu auprès de `oauth_token_url` puis mis en cache jusqu'à
//! son expiration. La forme du corps JSON de `/search` (champ `recherche`
//! avec `champs`/`criteres`/`fond`) suit le contrat documenté par le
//! catalogue d'API PISTE ; la forme exacte des routes `/consult/*`
//! mobilisées par `legifrance_fetch` dépend du fonds visé et doit être
//! confirmée contre le Swagger PISTE pour le fonds réellement utilisé.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::{
    error::ToolError,
    tool::{Tool, ToolOutput},
};

/// Configuration d'accès à l'API Légifrance via le portail PISTE.
#[derive(Debug, Clone)]
pub struct LegifranceConfig {
    /// Racine de l'API Légifrance (ex:
    /// `https://api.piste.gouv.fr/dila/legifrance/lf-engine-app`).
    pub api_base_url: String,
    /// Point de terminaison OAuth2 PISTE (ex:
    /// `https://oauth.piste.gouv.fr/api/oauth/token`).
    pub oauth_token_url: String,
    pub client_id: String,
    pub client_secret: String,
}

impl LegifranceConfig {
    #[must_use]
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            api_base_url: "https://api.piste.gouv.fr/dila/legifrance/lf-engine-app".to_string(),
            oauth_token_url: "https://oauth.piste.gouv.fr/api/oauth/token".to_string(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        }
    }
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    expires_in: u64,
}

struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

/// Client partagé entre les outils Légifrance : gère l'obtention et le
/// renouvellement du jeton OAuth2 PISTE.
pub struct LegifranceClient {
    http: reqwest::Client,
    config: LegifranceConfig,
    token: Mutex<Option<CachedToken>>,
}

impl LegifranceClient {
    #[must_use]
    pub fn new(config: LegifranceConfig) -> Self {
        Self { http: reqwest::Client::new(), config, token: Mutex::new(None) }
    }

    async fn access_token(&self) -> Result<String, ToolError> {
        let mut token = self.token.lock().await;

        if let Some(cached) = token.as_ref()
            && cached.expires_at > Instant::now()
        {
            return Ok(cached.access_token.clone());
        }

        let response = self
            .http
            .post(&self.config.oauth_token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("scope", "openid"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<OAuthTokenResponse>()
            .await?;

        // Marge de sécurité pour éviter d'utiliser un jeton expirant pendant la requête suivante.
        let expires_at = Instant::now() + Duration::from_secs(response.expires_in.saturating_sub(30));
        let access_token = response.access_token.clone();
        *token = Some(CachedToken { access_token: response.access_token, expires_at });

        Ok(access_token)
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value, ToolError> {
        let access_token = self.access_token().await?;

        let response = self
            .http
            .post(format!("{}{path}", self.config.api_base_url))
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        Ok(response)
    }
}

#[derive(Deserialize)]
struct SearchArguments {
    query: String,
    /// Fonds documentaire interrogé (ex: `"LODA_DATE"`, `"CODE_DATE"`,
    /// `"JURI"`, `"ALL"`).
    #[serde(default = "default_fond")]
    fond: String,
    #[serde(default = "default_max_results")]
    max_results: u32,
}

fn default_fond() -> String {
    "ALL".to_string()
}

fn default_max_results() -> u32 {
    10
}

/// Outil `legifrance_search` : recherche dans la base Légifrance (textes
/// législatifs, jurisprudence) via l'API officielle.
pub struct LegifranceSearchTool {
    client: std::sync::Arc<LegifranceClient>,
}

impl LegifranceSearchTool {
    #[must_use]
    pub fn new(client: std::sync::Arc<LegifranceClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for LegifranceSearchTool {
    fn name(&self) -> &str {
        "legifrance_search"
    }

    fn description(&self) -> &str {
        "Recherche dans la base Légifrance (textes législatifs, jurisprudence) via l'API officielle PISTE."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Termes de recherche" },
                "fond": {
                    "type": "string",
                    "description": "Fonds documentaire à interroger",
                    "enum": ["ALL", "LODA_DATE", "CODE_DATE", "JURI", "JORF", "CETAT"],
                    "default": "ALL"
                },
                "max_results": { "type": "integer", "description": "Nombre maximal de résultats", "default": 10 }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: SearchArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let body = json!({
            "recherche": {
                "champs": [{
                    "typeChamp": "ALL",
                    "criteres": [{ "typeRecherche": "UN_DES_MOTS", "valeur": args.query, "operateur": "ET" }],
                    "operateur": "ET",
                }],
                "pageNumber": 1,
                "pageSize": args.max_results,
                "sort": "PERTINENCE",
            },
            "fond": args.fond,
        });

        let response = self.client.post("/search", &body).await?;
        Ok(ToolOutput::new(response.to_string()))
    }
}

#[derive(Deserialize)]
struct FetchArguments {
    /// Identifiant du texte, tel que renvoyé par `legifrance_search` (champ
    /// `id` des résultats).
    id: String,
    /// Route de consultation PISTE correspondant au fonds du texte (ex:
    /// `"legiPart"` pour un texte LODA, `"juri"` pour une décision de
    /// jurisprudence, `"code"` pour un article de code).
    #[serde(default = "default_route")]
    route: String,
}

fn default_route() -> String {
    "legiPart".to_string()
}

/// Outil `legifrance_fetch` : récupère le contenu complet d'un texte par
/// identifiant.
pub struct LegifranceFetchTool {
    client: std::sync::Arc<LegifranceClient>,
}

impl LegifranceFetchTool {
    #[must_use]
    pub fn new(client: std::sync::Arc<LegifranceClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for LegifranceFetchTool {
    fn name(&self) -> &str {
        "legifrance_fetch"
    }

    fn description(&self) -> &str {
        "Récupère le contenu complet d'un texte Légifrance par identifiant (résultat de legifrance_search)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Identifiant du texte à récupérer" },
                "route": {
                    "type": "string",
                    "description": "Route de consultation PISTE correspondant au fonds du texte",
                    "enum": ["legiPart", "juri", "code", "jorf"],
                    "default": "legiPart"
                }
            },
            "required": ["id"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: FetchArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let response = self.client.post(&format!("/consult/{}", args.route), &json!({ "id": args.id })).await?;
        Ok(ToolOutput::new(response.to_string()))
    }
}
