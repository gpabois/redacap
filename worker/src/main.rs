#[tokio::main]
async fn main() -> anyhow::Result<()> {
    worker::run().await
}
