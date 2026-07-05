//! Page `/login` : authentification par identifiants ou par fournisseur
//! OpenID Connect (voir `Claude.md` § Authentification).
//!
//! La connexion elle-même n'est pas gérée par des fonctions serveur Leptos :
//! le formulaire d'identifiants soumet nativement vers `POST /login` et les
//! liens de fournisseurs OIDC pointent vers `GET /oidc/{id}/start`, deux
//! routes Axum simples exposées par `server::auth`. Cette page ne fait que
//! rendre le formulaire, la liste des fournisseurs actifs, et un message
//! d'erreur éventuel porté par le paramètre de requête `error`.

use dsfr::{Alert, Button, ButtonVariant, Input};
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde::{Deserialize, Serialize};

/// Fournisseur OIDC actif tel qu'exposé à cette page : uniquement ce qui est
/// nécessaire à l'affichage (jamais `client_secret_encrypted`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OidcProviderSummary {
    id: String,
    name: String,
}

#[server]
async fn list_active_oidc_providers() -> Result<Vec<OidcProviderSummary>, ServerFnError> {
    let pool = expect_context::<storage::Pool>();
    let providers = storage::oidc_provider::list_active_oidc_providers(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(providers
        .into_iter()
        .map(|provider| OidcProviderSummary {
            id: provider.id.to_string(),
            name: provider.name,
        })
        .collect())
}

/// Traduit le code d'erreur générique porté par `?error=...` (jamais de
/// détail distinguant email inconnu / mot de passe invalide, afin de ne pas
/// faciliter l'énumération de comptes — voir `server::auth::credentials`).
fn error_message(code: &str) -> &'static str {
    match code {
        "identifiants" => "Email ou mot de passe incorrect.",
        "oidc" => {
            "La connexion via ce fournisseur a échoué. Réessayez ou contactez un administrateur."
        }
        "indisponible" => "Le service d'authentification est temporairement indisponible.",
        _ => "La connexion a échoué. Réessayez.",
    }
}

#[component]
pub fn PageLogin() -> impl IntoView {
    let query = use_query_map();
    let error = move || query.read().get("error").map(|code| error_message(&code));

    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());

    let providers = Resource::new(|| (), |_| list_active_oidc_providers());

    view! {
        <div class="min-h-screen bg-gray-50 flex items-center justify-center p-6">
            <div class="w-full max-w-md bg-white border border-gray-200 rounded-sm p-8 flex flex-col gap-6">
                <div>
                    <h1 class="text-xl font-bold text-gray-900">"Connexion"</h1>
                    <p class="text-sm text-gray-600">"Éditeur d'arrêtés préfectoraux"</p>
                </div>

                {move || error().map(|message| view! {
                    <Alert severity=dsfr::components::common::Severity::Error small=true>
                        {message}
                    </Alert>
                })}

                <form method="post" action="/login" class="flex flex-col gap-4">
                    <Input
                        label="Email"
                        name="email"
                        r#type="email"
                        value=email
                        on_input=move |value| set_email.set(value)
                    />
                    <Input
                        label="Mot de passe"
                        name="password"
                        r#type="password"
                        value=password
                        on_input=move |value| set_password.set(value)
                    />
                    <Button r#type="submit" variant=ButtonVariant::Primary on_click=|_| {}>
                        "Se connecter"
                    </Button>
                </form>

                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match providers.await {
                            Ok(providers) if !providers.is_empty() => Some(view! {
                                <div class="flex flex-col gap-3">
                                    <div class="flex items-center gap-3 text-xs text-gray-500 uppercase">
                                        <span class="flex-1 border-t border-gray-200"></span>
                                        "ou"
                                        <span class="flex-1 border-t border-gray-200"></span>
                                    </div>
                                    <div class="flex flex-col gap-2">
                                        {providers.into_iter().map(|provider| view! {
                                            <a
                                                href=format!("/oidc/{}/start", provider.id)
                                                class="text-center bg-transparent text-blue-france shadow-[inset_0_0_0_1px] shadow-gray-300 hover:bg-blue-france-975 font-bold px-4 py-2 transition-colors"
                                            >
                                                {format!("Se connecter avec {}", provider.name)}
                                            </a>
                                        }).collect_view()}
                                    </div>
                                </div>
                            }),
                            _ => None,
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}
