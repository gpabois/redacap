#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::run().await
}
