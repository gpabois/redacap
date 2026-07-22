use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use legifrance_client::{
    LegifranceClient,
    models::{SearchRequestDTO, SearchResponseDTO},
};
use marie::{network::worker::JobContext, secret::SecretManager, tools::Toolable};
use serde_json::{Value, json};
use std::sync::Arc;

pub async fn create_client(
    pool: storage::Pool,
    secret: SecretManager,
) -> anyhow::Result<LegifranceClient> {
    let creds = storage::external_credentials::get_legifrance_credentials(&pool)
        .await?
        .ok_or_else(|| anyhow!("aucune configuration n'a été effectuée pour accéder à l'API Légifrance"))?;
    let client_id = creds
        .client_id
        .ok_or_else(|| anyhow!("le client_id Légifrance n'est pas configuré"))?;
    let encrypted_secret = creds
        .client_secret_encrypted
        .ok_or_else(|| anyhow!("le client_secret Légifrance n'est pas configuré"))?;
    let client_secret = super::secret::decrypt(&secret, &encrypted_secret)?;

    let client = LegifranceClient::builder(client_id, client_secret).build()?;
    Ok(client)
}

pub type LegifranceClientFactory = Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<LegifranceClient>> + Sync + Send + 'static>;


#[derive(Clone)]
pub struct SearchLegifrance(pub(crate) LegifranceClientFactory);

#[async_trait]
impl Toolable<JobContext> for SearchLegifrance {
    const NAME: &str = "search_legifrance";
    const DESCRIPTION: &str = "Recherche dans la base Légifrance (textes législatifs, jurisprudence) via l'API officielle PISTE.";

    type Args = SearchRequestDTO;
    type Return = SearchResponseDTO;

    fn parameters_schema() -> Value {
        json!({
            "type": "object",
            "required": ["recherche", "fond"],
            "properties": {
                "recherche": { "$ref": "#/$defs/RechercheSpecifiqueDTO" },
                "fond": {
                    "type": "string",
                    "description": "Fonds documentaire interrogé",
                    "enum": [
                        "JORF", "CNIL", "CETAT", "JURI", "JUFI", "CONSTIT", "KALI",
                        "CODE_DATE", "CODE_ETAT", "LODA_DATE", "LODA_ETAT", "ALL", "CIRC", "ACCO"
                    ]
                }
            },
            "$defs": {
                "RechercheSpecifiqueDTO": {
                    "type": "object",
                    "required": ["sort", "champs", "pageSize", "operateur", "typePagination", "pageNumber"],
                    "properties": {
                        "filtres": {
                            "type": "array",
                            "items": { "$ref": "#/$defs/FiltreDTO" }
                        },
                        "sort": { "type": "string", "description": "Critère de tri des résultats" },
                        "fromAdvancedRecherche": { "type": "boolean" },
                        "secondSort": { "type": "string" },
                        "champs": {
                            "type": "array",
                            "items": { "$ref": "#/$defs/ChampDTO" }
                        },
                        "pageSize": { "type": "integer" },
                        "operateur": { "type": "string", "enum": ["ET", "OU"] },
                        "typePagination": { "type": "string", "enum": ["DEFAUT", "ARTICLE"] },
                        "pageNumber": { "type": "integer" }
                    }
                },
                "ChampDTO": {
                    "type": "object",
                    "properties": {
                        "criteres": {
                            "type": "array",
                            "items": { "$ref": "#/$defs/CritereDTO" }
                        },
                        "operateur": { "type": "string", "enum": ["ET", "OU"] },
                        "typeChamp": {
                            "type": "string",
                            "enum": [
                                "ALL", "TITLE", "TABLE", "NOR", "NUM", "ADVANCED_TEXTE_ID",
                                "NUM_DELIB", "NUM_DEC", "NUM_ARTICLE", "ARTICLE", "MINISTERE",
                                "VISA", "NOTICE", "VISA_NOTICE", "TRAVAUX_PREP", "SIGNATURE",
                                "NOTA", "NUM_AFFAIRE", "ABSTRATS", "RESUMES", "TEXTE", "ECLI",
                                "NUM_LOI_DEF", "TYPE_DECISION", "NUMERO_INTERNE", "REF_PUBLI",
                                "RESUME_CIRC", "TEXTE_REF", "TITRE_LOI_DEF", "RAISON_SOCIALE",
                                "MOTS_CLES", "IDCC"
                            ]
                        }
                    }
                },
                "CritereDTO": {
                    "type": "object",
                    "required": ["valeur", "operateur", "typeRecherche"],
                    "properties": {
                        "proximite": { "type": "integer" },
                        "valeur": { "type": "string" },
                        "criteres": {
                            "type": "array",
                            "items": { "$ref": "#/$defs/CritereDTO" }
                        },
                        "operateur": { "type": "string", "enum": ["ET", "OU"] },
                        "typeRecherche": {
                            "type": "string",
                            "enum": [
                                "UN_DES_MOTS", "EXACTE", "TOUS_LES_MOTS_DANS_UN_CHAMP",
                                "AUCUN_DES_MOTS", "AUCUNE_CORRESPONDANCE_A_CETTE_EXPRESSION"
                            ]
                        }
                    }
                },
                "FiltreDTO": {
                    "type": "object",
                    "properties": {
                        "dates": { "$ref": "#/$defs/DatePeriod" },
                        "valeurs": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "singleDate": { "type": "string" },
                        "facette": { "type": "string" },
                        "multiValeurs": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        }
                    }
                },
                "DatePeriod": {
                    "type": "object",
                    "properties": {
                        "start": { "type": "string" },
                        "end": { "type": "string" }
                    }
                }
            }
        })
    }

    async fn execute(self, _cx: JobContext, args: Self::Args) -> anyhow::Result<Self::Return> {
        let client = (self.0)().await?;
        let resp = client.search(&args).await?;
        Ok(resp)
    }
}