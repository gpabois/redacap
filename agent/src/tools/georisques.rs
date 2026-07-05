//! Outils `georisques_query` et `icpe_query`, qui interrogent l'API
//! publique GéoRisques (`https://georisques.gouv.fr/api/v1`). Les
//! paramètres de requête sont transmis tels quels au point de terminaison
//! GéoRisques (passthrough) : le modèle choisit les clés pertinentes parmi
//! celles documentées dans le schéma de chaque outil, ce qui évite de figer
//! ici un sous-ensemble arbitraire des très nombreux filtres disponibles
//! côté GéoRisques.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::{
    error::ToolError,
    tool::{Tool, ToolOutput},
};

/// Configuration d'accès à l'API GéoRisques. Les API `v1` sont accessibles
/// sans jeton ; `api_key`, si fourni, est transmis en tant que jeton
/// porteur pour bénéficier d'un quota de requêtes plus élevé.
#[derive(Debug, Clone)]
pub struct GeorisquesConfig {
    pub base_url: String,
    pub api_key: Option<String>,
}

impl Default for GeorisquesConfig {
    fn default() -> Self {
        Self {
            base_url: "https://georisques.gouv.fr/api/v1".to_string(),
            api_key: None,
        }
    }
}

/// Client HTTP partagé entre les outils GéoRisques.
pub struct GeorisquesClient {
    http: reqwest::Client,
    config: GeorisquesConfig,
}

impl GeorisquesClient {
    #[must_use]
    pub fn new(config: GeorisquesConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    async fn get(&self, path: &str, params: &Map<String, Value>) -> Result<Value, ToolError> {
        let mut request = self
            .http
            .get(format!("{}{path}", self.config.base_url))
            .query(&query_pairs(params));

        if let Some(api_key) = &self.config.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        Ok(response)
    }
}

fn query_pairs(params: &Map<String, Value>) -> Vec<(String, String)> {
    params
        .iter()
        .filter_map(|(key, value)| scalar_to_string(value).map(|value| (key.clone(), value)))
        .collect()
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn object_arguments(arguments: Value) -> Result<Map<String, Value>, ToolError> {
    match arguments {
        Value::Object(map) => Ok(map),
        _ => Err(ToolError::InvalidArguments(
            "les arguments doivent être un objet JSON".to_string(),
        )),
    }
}

/// Outil `georisques_query` : interroge l'API GéoRisques pour les risques
/// naturels et technologiques connus au droit d'une installation (route
/// `/resultats_rapport_risque`).
pub struct GeorisquesQueryTool {
    client: Arc<GeorisquesClient>,
}

impl GeorisquesQueryTool {
    #[must_use]
    pub fn new(client: Arc<GeorisquesClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for GeorisquesQueryTool {
    fn name(&self) -> &str {
        "georisques_query"
    }

    fn description(&self) -> &str {
        "Interroge l'API GéoRisques (rapport de risques naturels et technologiques) pour une localisation donnée."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "code_insee": { "type": "string", "description": "Code INSEE de la commune de l'installation" },
                "latlon": { "type": "string", "description": "Coordonnées \"longitude,latitude\" de l'installation" },
                "rayon": { "type": "integer", "description": "Rayon de recherche en mètres" },
                "page": { "type": "integer", "default": 1 },
                "page_size": { "type": "integer", "default": 10 }
            }
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let params = object_arguments(arguments)?;
        let response = self
            .client
            .get("/resultats_rapport_risque", &params)
            .await?;
        Ok(ToolOutput::new(response.to_string()))
    }
}

/// Outil `icpe_query` : interroge la base des installations classées
/// (ICPE/AIOT) pour les données administratives d'un établissement (route
/// `/installations_classees`).
pub struct IcpeQueryTool {
    client: Arc<GeorisquesClient>,
}

impl IcpeQueryTool {
    #[must_use]
    pub fn new(client: Arc<GeorisquesClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for IcpeQueryTool {
    fn name(&self) -> &str {
        "icpe_query"
    }

    fn description(&self) -> &str {
        "Interroge la base ICPE/AIOT (via l'API GéoRisques) pour les données administratives d'un établissement \
         (raison sociale, adresse, rubriques, régime, statut Seveso...)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "nom_etablissement": { "type": "string", "description": "Raison sociale, recherche partielle" },
                "code_postal": { "type": "string" },
                "departement": { "type": "string", "description": "Code département (ex: \"33\")" },
                "region": { "type": "string", "description": "Code région INSEE" },
                "code_insee": { "type": "string", "description": "Code INSEE de la commune" },
                "regime": { "type": "string", "description": "Régime ICPE (ex: \"Autorisation\", \"Déclaration\")" },
                "code_aiot": { "type": "string", "description": "Code AIOT de l'installation" },
                "page": { "type": "integer", "default": 1 },
                "page_size": { "type": "integer", "default": 10 }
            }
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let params = object_arguments(arguments)?;
        let response = self.client.get("/installations_classees", &params).await?;
        Ok(ToolOutput::new(response.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_to_string_accepts_string_number_and_bool() {
        assert_eq!(
            scalar_to_string(&Value::String("33".to_string())),
            Some("33".to_string())
        );
        assert_eq!(
            scalar_to_string(&serde_json::json!(10)),
            Some("10".to_string())
        );
        assert_eq!(
            scalar_to_string(&Value::Bool(true)),
            Some("true".to_string())
        );
    }

    #[test]
    fn scalar_to_string_rejects_composite_values() {
        assert_eq!(scalar_to_string(&Value::Null), None);
        assert_eq!(scalar_to_string(&serde_json::json!([1, 2])), None);
        assert_eq!(scalar_to_string(&serde_json::json!({ "a": 1 })), None);
    }

    #[test]
    fn query_pairs_filters_out_non_scalar_entries() {
        let params = serde_json::json!({
            "code_insee": "33063",
            "rayon": 500,
            "ignored": { "nested": true },
        })
        .as_object()
        .unwrap()
        .clone();

        let mut pairs = query_pairs(&params);
        pairs.sort();

        assert_eq!(
            pairs,
            vec![
                ("code_insee".to_string(), "33063".to_string()),
                ("rayon".to_string(), "500".to_string()),
            ]
        );
    }

    #[test]
    fn object_arguments_accepts_json_object() {
        let arguments = serde_json::json!({ "code_insee": "33063" });
        let params = object_arguments(arguments).unwrap();
        assert_eq!(params.get("code_insee").unwrap(), "33063");
    }

    #[test]
    fn object_arguments_rejects_non_object() {
        let error = object_arguments(Value::String("invalide".to_string())).unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments(_)));
    }
}
