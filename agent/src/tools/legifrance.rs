//! Outils `legifrance_search` et `legifrance_fetch`, qui interrogent l'API
//! officielle Légifrance sur le portail PISTE (`lf-engine-app`, v2).
//!
//! L'authentification PISTE est de type OAuth2 `client_credentials` : le
//! jeton est obtenu auprès de `oauth_token_url` puis mis en cache jusqu'à
//! son expiration. Les corps de requête (`/search`, `/consult/*`) et leurs
//! champs suivent le contrat documenté par le Swagger officiel de l'API
//! Légifrance v2 (DTOs `SearchRequestDTO`, `LegiConsultRequest`,
//! `CodeConsultRequest`, `JuriConsultRequest`, `JorfConsultRequest`,
//! `ArticleRequest`).

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
        Self {
            http: reqwest::Client::new(),
            config,
            token: Mutex::new(None),
        }
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
        let expires_at =
            Instant::now() + Duration::from_secs(response.expires_in.saturating_sub(30));
        let access_token = response.access_token.clone();
        *token = Some(CachedToken {
            access_token: response.access_token,
            expires_at,
        });

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

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[derive(Deserialize)]
struct SearchArguments {
    query: String,
    /// Fonds documentaire interrogé (ex: `"LODA_DATE"`, `"CODE_DATE"`,
    /// `"JURI"`, `"ALL"`).
    #[serde(default = "default_fond")]
    fond: String,
    /// Champ dans lequel porte la recherche (`ALL`, `TITLE`, `ARTICLE`,
    /// `NOR`, `NUM`...).
    #[serde(default = "default_type_champ")]
    type_champ: String,
    /// Mode de correspondance des mots de `query` (voir `CritereDTO`).
    #[serde(default = "default_type_recherche")]
    type_recherche: String,
    /// Critère de tri des résultats (ex: `"PERTINENCE"`,
    /// `"SIGNATURE_DATE_DESC"`).
    #[serde(default = "default_sort")]
    sort: String,
    #[serde(default = "default_page_number")]
    page_number: u32,
    #[serde(default = "default_page_size")]
    page_size: u32,
    /// Restreint la recherche à ces natures de texte (filtre facette
    /// `NATURE`, ex: `["ARRETE", "DECRET"]`).
    #[serde(default)]
    nature: Vec<String>,
    /// Borne inférieure (incluse) de la date de signature, au format
    /// `AAAA-MM-JJ` (filtre facette `DATE_SIGNATURE`).
    #[serde(default)]
    date_signature_debut: Option<String>,
    /// Borne supérieure (incluse) de la date de signature, au format
    /// `AAAA-MM-JJ` (filtre facette `DATE_SIGNATURE`).
    #[serde(default)]
    date_signature_fin: Option<String>,
}

fn default_fond() -> String {
    "ALL".to_string()
}

fn default_type_champ() -> String {
    "ALL".to_string()
}

fn default_type_recherche() -> String {
    "TOUS_LES_MOTS_DANS_UN_CHAMP".to_string()
}

fn default_sort() -> String {
    "PERTINENCE".to_string()
}

fn default_page_number() -> u32 {
    1
}

fn default_page_size() -> u32 {
    10
}

fn nature_filter(nature: &[String]) -> Option<Value> {
    if nature.is_empty() {
        return None;
    }
    Some(json!({ "facette": "NATURE", "valeurs": nature }))
}

fn date_signature_filter(debut: Option<&str>, fin: Option<&str>) -> Option<Value> {
    if debut.is_none() && fin.is_none() {
        return None;
    }
    Some(json!({
        "facette": "DATE_SIGNATURE",
        "dates": { "start": debut, "end": fin },
    }))
}

fn build_search_body(args: &SearchArguments) -> Value {
    let filtres: Vec<Value> = [
        nature_filter(&args.nature),
        date_signature_filter(
            args.date_signature_debut.as_deref(),
            args.date_signature_fin.as_deref(),
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut recherche = json!({
        "champs": [{
            "typeChamp": args.type_champ,
            "operateur": "ET",
            "criteres": [{
                "typeRecherche": args.type_recherche,
                "valeur": args.query,
                "operateur": "ET",
            }],
        }],
        "operateur": "ET",
        "pageNumber": args.page_number,
        "pageSize": args.page_size,
        "sort": args.sort,
        "typePagination": "DEFAUT",
    });
    if !filtres.is_empty() {
        recherche["filtres"] = Value::Array(filtres);
    }

    json!({ "fond": args.fond, "recherche": recherche })
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
                    "enum": [
                        "ALL", "LODA_DATE", "LODA_ETAT", "CODE_DATE", "CODE_ETAT",
                        "JURI", "JORF", "CETAT", "CONSTIT", "KALI", "CIRC", "CNIL", "ACCO"
                    ],
                    "default": "ALL"
                },
                "type_champ": {
                    "type": "string",
                    "description": "Champ dans lequel porte la recherche",
                    "enum": ["ALL", "TITLE", "TABLE", "NOR", "NUM", "ARTICLE", "NUM_ARTICLE", "VISA", "NOTA", "TEXTE"],
                    "default": "ALL"
                },
                "type_recherche": {
                    "type": "string",
                    "description": "Mode de correspondance des mots de `query`",
                    "enum": [
                        "UN_DES_MOTS", "TOUS_LES_MOTS_DANS_UN_CHAMP", "EXACTE",
                        "AUCUN_DES_MOTS", "AUCUNE_CORRESPONDANCE_A_CETTE_EXPRESSION"
                    ],
                    "default": "TOUS_LES_MOTS_DANS_UN_CHAMP"
                },
                "sort": {
                    "type": "string",
                    "description": "Critère de tri des résultats",
                    "default": "PERTINENCE"
                },
                "page_number": { "type": "integer", "description": "Numéro de page (à partir de 1)", "default": 1 },
                "page_size": { "type": "integer", "description": "Nombre de résultats par page (max 100)", "default": 10 },
                "nature": {
                    "type": "array",
                    "description": "Restreint aux natures de texte indiquées (ex: [\"ARRETE\"])",
                    "items": { "type": "string" }
                },
                "date_signature_debut": { "type": "string", "description": "Date de signature minimale, format AAAA-MM-JJ" },
                "date_signature_fin": { "type": "string", "description": "Date de signature maximale, format AAAA-MM-JJ" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: SearchArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let body = build_search_body(&args);
        let response = self.client.post("/search", &body).await?;
        Ok(ToolOutput::new(response.to_string()))
    }
}

/// Arguments de `legifrance_fetch`, discriminés par `route` : chaque route
/// correspond à un point de terminaison `/consult/*` de l'API Légifrance et
/// impose ses propres identifiants (voir le Swagger officiel).
#[derive(Deserialize)]
#[serde(tag = "route")]
enum FetchArguments {
    /// `/consult/legiPart` : texte du fonds LEGI (lois, décrets, arrêtés),
    /// identifié par son Chronical ID et une date de vigueur.
    #[serde(rename = "legiPart")]
    LegiPart {
        #[serde(rename = "textId")]
        text_id: String,
        #[serde(default = "today")]
        date: String,
    },
    /// `/consult/code` : article ou section d'un code, identifié par le
    /// Chronical ID du code et une date de vigueur ; `sctCid` cible une
    /// section précise.
    #[serde(rename = "code")]
    Code {
        #[serde(rename = "textId")]
        text_id: String,
        #[serde(default = "today")]
        date: String,
        #[serde(rename = "sctCid", default)]
        sct_cid: Option<String>,
    },
    /// `/consult/juri` : décision de jurisprudence, identifiée par son
    /// identifiant `JURITEXT...`.
    #[serde(rename = "juri")]
    Juri {
        #[serde(rename = "textId")]
        text_id: String,
    },
    /// `/consult/jorf` : texte publié au Journal officiel, identifié par son
    /// Chronical ID `JORFTEXT...`.
    #[serde(rename = "jorf")]
    Jorf {
        #[serde(rename = "textCid")]
        text_cid: String,
    },
    /// `/consult/getArticle` : contenu d'un article isolé, identifié par son
    /// identifiant `LEGIARTI...`.
    #[serde(rename = "getArticle")]
    GetArticle { id: String },
}

fn build_fetch_request(args: &FetchArguments) -> (&'static str, Value) {
    match args {
        FetchArguments::LegiPart { text_id, date } => (
            "/consult/legiPart",
            json!({ "textId": text_id, "date": date }),
        ),
        FetchArguments::Code {
            text_id,
            date,
            sct_cid,
        } => {
            let mut body = json!({ "textId": text_id, "date": date });
            if let Some(sct_cid) = sct_cid {
                body["sctCid"] = json!(sct_cid);
            }
            ("/consult/code", body)
        }
        FetchArguments::Juri { text_id } => {
            ("/consult/juri", json!({ "textId": text_id }))
        }
        FetchArguments::Jorf { text_cid } => {
            ("/consult/jorf", json!({ "textCid": text_cid }))
        }
        FetchArguments::GetArticle { id } => {
            ("/consult/getArticle", json!({ "id": id }))
        }
    }
}

/// Outil `legifrance_fetch` : récupère le contenu complet d'un texte ou
/// d'un article par identifiant (résultat de `legifrance_search`).
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
        "Récupère le contenu complet d'un texte ou d'un article Légifrance par identifiant \
         (résultat de legifrance_search). La route à utiliser dépend du fonds du texte : \
         `legiPart` pour un texte LODA (loi, décret, arrêté), `code` pour un article/section \
         de code, `juri` pour une décision de jurisprudence, `jorf` pour un texte JORF, \
         `getArticle` pour un article isolé (identifiant LEGIARTI...)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "route": {
                    "type": "string",
                    "description": "Point de terminaison de consultation à utiliser, selon le fonds du texte",
                    "enum": ["legiPart", "code", "juri", "jorf", "getArticle"]
                },
                "textId": {
                    "type": "string",
                    "description": "Chronical ID du texte (requis pour les routes legiPart, code, juri)"
                },
                "date": {
                    "type": "string",
                    "description": "Date de vigueur, format AAAA-MM-JJ (routes legiPart et code ; défaut : aujourd'hui)"
                },
                "sctCid": {
                    "type": "string",
                    "description": "Chronical ID de la section du code à consulter (route code, optionnel)"
                },
                "textCid": {
                    "type": "string",
                    "description": "Chronical ID du texte JORF (requis pour la route jorf)"
                },
                "id": {
                    "type": "string",
                    "description": "Identifiant de l'article (requis pour la route getArticle, ex: LEGIARTI...)"
                }
            },
            "required": ["route"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: FetchArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let (path, body) = build_fetch_request(&args);
        let response = self.client.post(path, &body).await?;
        Ok(ToolOutput::new(response.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_body_carries_required_fields() {
        let args = SearchArguments {
            query: "installation classée".to_string(),
            fond: default_fond(),
            type_champ: default_type_champ(),
            type_recherche: default_type_recherche(),
            sort: default_sort(),
            page_number: default_page_number(),
            page_size: default_page_size(),
            nature: Vec::new(),
            date_signature_debut: None,
            date_signature_fin: None,
        };

        let body = build_search_body(&args);

        assert_eq!(body["fond"], "ALL");
        assert_eq!(body["recherche"]["typePagination"], "DEFAUT");
        assert_eq!(body["recherche"]["pageNumber"], 1);
        assert_eq!(
            body["recherche"]["champs"][0]["criteres"][0]["valeur"],
            "installation classée"
        );
        assert!(body["recherche"].get("filtres").is_none());
    }

    #[test]
    fn search_body_includes_nature_and_date_filters() {
        let args = SearchArguments {
            query: "seveso".to_string(),
            fond: default_fond(),
            type_champ: default_type_champ(),
            type_recherche: default_type_recherche(),
            sort: default_sort(),
            page_number: default_page_number(),
            page_size: default_page_size(),
            nature: vec!["ARRETE".to_string()],
            date_signature_debut: Some("2020-01-01".to_string()),
            date_signature_fin: None,
        };

        let body = build_search_body(&args);
        let filtres = body["recherche"]["filtres"].as_array().unwrap();

        assert!(
            filtres
                .iter()
                .any(|filtre| filtre["facette"] == "NATURE" && filtre["valeurs"][0] == "ARRETE")
        );
        assert!(filtres.iter().any(|filtre| filtre["facette"]
            == "DATE_SIGNATURE"
            && filtre["dates"]["start"] == "2020-01-01"));
    }

    #[test]
    fn fetch_request_dispatches_legi_part_route() {
        let args = FetchArguments::LegiPart {
            text_id: "JORFTEXT000000882738".to_string(),
            date: "2021-04-15".to_string(),
        };

        let (path, body) = build_fetch_request(&args);

        assert_eq!(path, "/consult/legiPart");
        assert_eq!(body["textId"], "JORFTEXT000000882738");
        assert_eq!(body["date"], "2021-04-15");
    }

    #[test]
    fn fetch_request_dispatches_code_route_with_optional_section() {
        let args = FetchArguments::Code {
            text_id: "LEGITEXT000006074220".to_string(),
            date: "2021-04-15".to_string(),
            sct_cid: Some("LEGISCTA000006159510".to_string()),
        };

        let (path, body) = build_fetch_request(&args);

        assert_eq!(path, "/consult/code");
        assert_eq!(body["sctCid"], "LEGISCTA000006159510");
    }

    #[test]
    fn fetch_request_dispatches_get_article_route() {
        let args = FetchArguments::GetArticle {
            id: "LEGIARTI000006307920".to_string(),
        };

        let (path, body) = build_fetch_request(&args);

        assert_eq!(path, "/consult/getArticle");
        assert_eq!(body["id"], "LEGIARTI000006307920");
    }

    #[test]
    fn fetch_arguments_deserialize_by_route_tag() {
        let value = json!({
            "route": "jorf",
            "textCid": "JORFTEXT000033736934",
        });

        let args: FetchArguments = serde_json::from_value(value).unwrap();
        let (path, body) = build_fetch_request(&args);

        assert_eq!(path, "/consult/jorf");
        assert_eq!(body["textCid"], "JORFTEXT000033736934");
    }
}
