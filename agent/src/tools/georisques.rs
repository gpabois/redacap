use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use georisques_client::{GeorisquesClient, api::installations_classees::InstallationClasseeModelWithDetailsV2};
use marie::{network::worker::JobContext, secret::SecretManager, tools::Toolable};
use serde_json::{Value, json};
use std::sync::Arc;

pub async fn create_client(
    pool: storage::Pool,
    secret: SecretManager,
) -> anyhow::Result<GeorisquesClient> {
    let creds = storage::external_credentials::get_georisques_credentials(&pool)
        .await?
        .ok_or_else(|| anyhow!("aucune configuration n'a été effectuée pour accéder à l'API Géorisques"))?;
    let encrypted_api_key = creds
        .api_key_encrypted
        .ok_or_else(|| anyhow!("aucune clé API Géorisques n'est configurée"))?;
    let api_key = super::secret::decrypt(&secret, &encrypted_api_key)?;

    let client = GeorisquesClient::new(api_key)?;
    Ok(client)
}

pub type GeorisquesClientFactory = Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<GeorisquesClient>> + Sync + Send + 'static>;

#[derive(Clone)]
pub struct GetAiot(pub(crate) GeorisquesClientFactory);

#[async_trait]
impl Toolable<JobContext> for GetAiot {
    const NAME: &str = "get-aiot";
    const DESCRIPTION: &str = "Récupère une installation classée à partir de son code AIOT.";
    
    type Args = String;
    type Return = InstallationClasseeModelWithDetailsV2;

    fn parameters_schema() -> Value {
        json!({
            "type": "string",
            "description": "Code AIOT de l'installation classée à récupérer"
        })
    }

    async fn execute(self, _: JobContext, code_aiot: Self::Args) -> anyhow::Result<Self::Return> {
        let factory = self.0;
        let client = factory().await?;
        let aiot = client.installation_classee(&code_aiot).await?;
        Ok(aiot)
    }
}