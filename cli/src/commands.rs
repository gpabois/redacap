//! Implémentation des sous-commandes du CLI d'administration.

use clap::{Args, Subcommand};
use shared::id::ID;
use shared::model::{CreatePermission, CreateUser, Permission, ResourceScope, Subject, User};
use storage::Pool;

/// Gestion des migrations de la base de données applicative.
#[derive(Subcommand)]
pub enum StorageCommand {
    /// Applique les migrations en attente.
    Migrate,
    /// Annule les migrations appliquées jusqu'à (et en excluant) la version cible.
    Revert {
        /// Version à atteindre après annulation (0 = annule toutes les migrations).
        #[arg(long, default_value_t = 0)]
        target: i64,
    },
}

/// Gestion des comptes utilisateurs et de leurs droits.
#[derive(Subcommand)]
pub enum AccountCommand {
    /// Crée un compte utilisateur avec authentification par identifiants (email + mot de passe).
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        display_name: String,
        /// Mot de passe en clair ; à défaut, une saisie interactive masquée est demandée.
        #[arg(long)]
        password: Option<String>,
    },
    /// Suspend un compte utilisateur sans le supprimer.
    Suspend {
        #[arg(long)]
        email: String,
    },
    /// Réactive un compte utilisateur préalablement suspendu.
    Reactivate {
        #[arg(long)]
        email: String,
    },
    /// Liste les comptes utilisateurs.
    List,
    /// Accorde une permission (sujet/ressource/action) à un utilisateur ou un groupe.
    Grant(GrantArgs),
    /// Révoque une permission.
    Revoke {
        #[arg(long)]
        permission_id: ID,
    },
    /// Liste les permissions directement accordées à un utilisateur ou un groupe.
    ListPermissions(SubjectArgs),
}

/// Désigne le titulaire d'une permission : soit un utilisateur, soit un groupe.
#[derive(Args)]
pub struct SubjectArgs {
    /// Email de l'utilisateur titulaire.
    #[arg(long, conflicts_with = "group_id")]
    user_email: Option<String>,
    /// Identifiant du groupe titulaire.
    #[arg(long, conflicts_with = "user_email")]
    group_id: Option<ID>,
}

#[derive(Args)]
pub struct GrantArgs {
    #[command(flatten)]
    subject: SubjectArgs,
    /// Type de ressource concerné (ex. "legal_act", "authority"...).
    #[arg(long)]
    resource_type: String,
    /// Action accordée (ex. "read", "write", "administrateur"...).
    #[arg(long)]
    action: String,
    /// Droit circonscrit à une ressource précise.
    #[arg(long, conflicts_with_all = ["resource_group_id", "global"])]
    resource_id: Option<ID>,
    /// Droit sur toute ressource gérée par le groupe désigné.
    #[arg(long, conflicts_with_all = ["resource_id", "global"])]
    resource_group_id: Option<ID>,
    /// Droit global, non circonscrit à une ressource précise.
    #[arg(long, conflicts_with_all = ["resource_id", "resource_group_id"])]
    global: bool,
}

/// Lance le serveur applicatif (SSR + API privée/publique).
pub async fn run_server() -> anyhow::Result<()> {
    server::run().await
}

/// Lance un worker de traitement asynchrone des tâches longues.
pub async fn run_worker() -> anyhow::Result<()> {
    worker::run().await
}

/// Exécute une sous-commande de gestion des migrations.
pub async fn run_storage(command: StorageCommand, database_url: &str) -> anyhow::Result<()> {
    let pool = storage::connect(database_url).await?;
    match command {
        StorageCommand::Migrate => {
            storage::migrate(&pool).await?;
            println!("migrations appliquées");
        }
        StorageCommand::Revert { target } => {
            storage::revert(&pool, target).await?;
            println!("migrations annulées jusqu'à la version {target}");
        }
    }
    Ok(())
}

/// Exécute une sous-commande de gestion des comptes et des droits.
pub async fn run_account(command: AccountCommand, database_url: &str) -> anyhow::Result<()> {
    let pool = storage::connect(database_url).await?;
    match command {
        AccountCommand::Create {
            email,
            display_name,
            password,
        } => create_account(&pool, email, display_name, password).await,
        AccountCommand::Suspend { email } => {
            let user = storage::user::get_user_by_email(&pool, &email).await?;
            storage::user::suspend_user(&pool, &user.id).await?;
            println!("compte suspendu : {email}");
            Ok(())
        }
        AccountCommand::Reactivate { email } => {
            let user = storage::user::get_user_by_email(&pool, &email).await?;
            storage::user::reactivate_user(&pool, &user.id).await?;
            println!("compte réactivé : {email}");
            Ok(())
        }
        AccountCommand::List => list_accounts(&pool).await,
        AccountCommand::Grant(args) => grant_permission(&pool, args).await,
        AccountCommand::Revoke { permission_id } => {
            storage::permission::delete_permission(&pool, &permission_id).await?;
            println!("permission révoquée : {permission_id}");
            Ok(())
        }
        AccountCommand::ListPermissions(args) => list_permissions(&pool, args).await,
    }
}

async fn create_account(
    pool: &Pool,
    email: String,
    display_name: String,
    password: Option<String>,
) -> anyhow::Result<()> {
    let password = match password {
        Some(password) => password,
        None => rpassword::prompt_password("Mot de passe : ")?,
    };
    let user: User = storage::user::create_user(
        pool,
        CreateUser {
            email,
            display_name,
        },
    )
    .await?;
    storage::credential::set_password(pool, &user.id, &password).await?;
    println!("compte créé : {} ({})", user.email, user.id);
    Ok(())
}

async fn list_accounts(pool: &Pool) -> anyhow::Result<()> {
    for user in storage::user::list_users(pool).await? {
        let status = if user.suspended_at.is_some() {
            "suspendu"
        } else {
            "actif"
        };
        println!(
            "{}\t{}\t{}\t{status}",
            user.id, user.email, user.display_name
        );
    }
    Ok(())
}

async fn grant_permission(pool: &Pool, args: GrantArgs) -> anyhow::Result<()> {
    let subject = resolve_subject(pool, args.subject).await?;
    let resource = resolve_resource(args.resource_id, args.resource_group_id, args.global)?;
    let permission: Permission = storage::permission::create_permission(
        pool,
        CreatePermission {
            subject,
            resource_type: args.resource_type,
            resource,
            action: args.action,
        },
    )
    .await?;
    println!("permission accordée : {}", permission.id);
    Ok(())
}

async fn list_permissions(pool: &Pool, args: SubjectArgs) -> anyhow::Result<()> {
    let subject = resolve_subject(pool, args).await?;
    let permissions = match subject {
        Subject::User(user_id) => {
            storage::permission::list_permissions_for_user(pool, &user_id).await?
        }
        Subject::Group(group_id) => {
            storage::permission::list_permissions_for_group(pool, &group_id).await?
        }
    };
    for permission in permissions {
        println!(
            "{}\t{}\t{}",
            permission.id, permission.resource_type, permission.action
        );
    }
    Ok(())
}

async fn resolve_subject(pool: &Pool, args: SubjectArgs) -> anyhow::Result<Subject> {
    match (args.user_email, args.group_id) {
        (Some(email), None) => {
            let user = storage::user::get_user_by_email(pool, &email).await?;
            Ok(Subject::User(user.id))
        }
        (None, Some(group_id)) => Ok(Subject::Group(group_id)),
        _ => anyhow::bail!("préciser exactement l'un de --user-email ou --group-id"),
    }
}

fn resolve_resource(
    resource_id: Option<ID>,
    resource_group_id: Option<ID>,
    global: bool,
) -> anyhow::Result<ResourceScope> {
    match (resource_id, resource_group_id, global) {
        (Some(id), None, false) => Ok(ResourceScope::Specific(id)),
        (None, Some(group_id), false) => Ok(ResourceScope::ManagedByGroup(group_id)),
        (None, None, true) => Ok(ResourceScope::Global),
        _ => {
            anyhow::bail!(
                "préciser exactement l'un de --resource-id, --resource-group-id ou --global"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_resource_accepts_exactly_one_scope() {
        let id = shared::id::generate_id();
        assert!(matches!(
            resolve_resource(Some(id), None, false),
            Ok(ResourceScope::Specific(specific_id)) if specific_id == id
        ));
        assert!(matches!(
            resolve_resource(None, Some(id), false),
            Ok(ResourceScope::ManagedByGroup(group_id)) if group_id == id
        ));
        assert!(matches!(
            resolve_resource(None, None, true),
            Ok(ResourceScope::Global)
        ));
    }

    #[test]
    fn resolve_resource_rejects_ambiguous_or_empty_scope() {
        let id = shared::id::generate_id();
        assert!(resolve_resource(None, None, false).is_err());
        assert!(resolve_resource(Some(id), Some(id), false).is_err());
        assert!(resolve_resource(Some(id), None, true).is_err());
    }
}
