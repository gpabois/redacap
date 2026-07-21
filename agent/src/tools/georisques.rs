use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use georisques_client::{GeorisquesClient, GeorisquesClientBuilder, api::installations_classees::InstallationClasseeModelWithDetailsV2};
use marie::{network::worker::JobContext, tools::Toolable};
use serde_json::{Value, json};

pub async fn create_client(pool: storage::Pool) -> anyhow::Result<GeorisquesClient> {
    let creds = storage::external_credentials::get_georisques_credentials(&pool)
        .await?
        .ok_or(anyhow!("aucune configuration n'a été effectuée pour accéder à l'API Géorisques"))?;


    let client =  georisques_client::GeorisquesClient::new(creds.api_key_encrypted);
}

#[derive(Clone)]
pub struct GetAiot(Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<GeorisquesClient>>>);

#[async_trait]
impl Toolable<JobContext> for GetAiot {
    const NAME: &str = "get-aiot";
    const DESCRIPTION: &str = "Récupère une installation classée à partir de son code AIOT.";
    
    type Args = String;
    type Return = InstallationClasseeModelWithDetailsV2;

    fn parameters_schema() -> Value {
        json!({ "type": "string" })
    }

    async fn execute(self, _: JobContext, code_aiot: Self::Args) -> anyhow::Result<Self::Return> {
        let client = self.0;
        let aiot = client.installation_classee(&code_aiot).await?;
        Ok(aiot)
    }
}