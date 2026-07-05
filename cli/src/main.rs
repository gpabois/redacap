//! CLI d'administration de Redac'Ap : lancement des processus applicatifs,
//! gestion des migrations de la base et administration des comptes/droits.

mod commands;

use clap::{Parser, Subcommand};

use commands::{AccountCommand, StorageCommand};

#[derive(Parser)]
#[command(
    name = "redacap",
    version,
    about = "Outils d'administration de Redac'Ap"
)]
struct Cli {
    /// URL de connexion à la base Postgres (défaut : variable d'environnement DATABASE_URL).
    #[arg(long, global = true)]
    database_url: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lance le serveur applicatif (SSR + API).
    Server,
    /// Lance un worker de traitement asynchrone des tâches longues.
    Worker,
    /// Gestion des migrations de la base de données.
    Storage {
        #[command(subcommand)]
        command: StorageCommand,
    },
    /// Gestion des comptes utilisateurs et de leurs droits.
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    match cli.command {
        Command::Server => commands::run_server().await,
        Command::Worker => commands::run_worker().await,
        Command::Storage { command } => {
            let database_url = resolve_database_url(cli.database_url)?;
            commands::run_storage(command, &database_url).await
        }
        Command::Account { command } => {
            let database_url = resolve_database_url(cli.database_url)?;
            commands::run_account(command, &database_url).await
        }
    }
}

/// Résout l'URL de connexion à la base depuis `--database-url` ou `DATABASE_URL`.
fn resolve_database_url(database_url: Option<String>) -> anyhow::Result<String> {
    database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .ok_or_else(|| {
            anyhow::anyhow!("DATABASE_URL non défini (variable d'environnement ou --database-url)")
        })
}
