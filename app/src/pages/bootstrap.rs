//! Page `/bootstrap` : création du compte super administrateur unique tant
//! qu'aucun n'existe encore (voir `Claude.md` § « Ajoute un état bootstrap... »).
//!
//! Comme `/login` (voir `app::pages::login`), le formulaire soumet nativement
//! vers `POST /bootstrap`, une route Axum simple exposée par
//! `server::auth::bootstrap` : cette page ne fait que rendre le formulaire et
//! un message d'erreur éventuel porté par le paramètre `error`. Tant que
//! l'état bootstrap est actif, `server::guard::bootstrap_guard` redirige déjà
//! toute autre page ici ; [`bootstrap_status`] permet en plus de détecter
//! l'état déjà terminé (ex. onglet resté ouvert après la création du compte).

use dsfr::{Alert, Button, ButtonVariant, Input};
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

#[server]
async fn bootstrap_status() -> Result<bool, ServerFnError> {
    let pool = expect_context::<storage::Pool>();
    storage::bootstrap::is_required(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

/// Traduit le code d'erreur générique porté par `?error=...`.
fn error_message(code: &str) -> &'static str {
    match code {
        "champs" => "L'email et le nom sont obligatoires.",
        "mot_de_passe" => {
            "Le mot de passe doit contenir au moins 12 caractères et être confirmé à l'identique."
        }
        "indisponible" => "Le service est temporairement indisponible.",
        _ => "La création du compte a échoué. Réessayez.",
    }
}

#[component]
pub fn PageBootstrap() -> impl IntoView {
    let query = use_query_map();
    let error = move || query.read().get("error").map(|code| error_message(&code));

    let (email, set_email) = signal(String::new());
    let (display_name, set_display_name) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (password_confirmation, set_password_confirmation) = signal(String::new());

    let status = Resource::new(|| (), |_| bootstrap_status());

    view! {
        <div class="min-h-screen bg-gray-50 flex items-center justify-center p-6">
            <div class="w-full max-w-md bg-white border border-gray-200 rounded-sm p-8 flex flex-col gap-6">
                <div>
                    <h1 class="text-xl font-bold text-gray-900">"Initialisation"</h1>
                    <p class="text-sm text-gray-600">
                        "Créez le compte super administrateur unique de l'application."
                    </p>
                </div>

                {move || error().map(|message| view! {
                    <Alert severity=dsfr::components::common::Severity::Error small=true>
                        {message}
                    </Alert>
                })}

                <Suspense fallback=|| view! { <p class="text-sm text-gray-500">"Chargement…"</p> }>
                    {move || Suspend::new(async move {
                        match status.await {
                            Ok(false) => view! {
                                <div class="flex flex-col gap-3">
                                    <Alert severity=dsfr::components::common::Severity::Info small=true>
                                        "Un super administrateur existe déjà."
                                    </Alert>
                                    <a href="/login" class="text-sm text-blue-france underline">
                                        "Se connecter"
                                    </a>
                                </div>
                            }.into_any(),
                            _ => view! {
                                <form method="post" action="/bootstrap" class="flex flex-col gap-4">
                                    <Input
                                        label="Email"
                                        name="email"
                                        r#type="email"
                                        value=email
                                        on_input=move |value| set_email.set(value)
                                    />
                                    <Input
                                        label="Nom affiché"
                                        name="display_name"
                                        r#type="text"
                                        value=display_name
                                        on_input=move |value| set_display_name.set(value)
                                    />
                                    <Input
                                        label="Mot de passe"
                                        name="password"
                                        r#type="password"
                                        value=password
                                        on_input=move |value| set_password.set(value)
                                    />
                                    <Input
                                        label="Confirmation du mot de passe"
                                        name="password_confirmation"
                                        r#type="password"
                                        value=password_confirmation
                                        on_input=move |value| set_password_confirmation.set(value)
                                    />
                                    <Button r#type="submit" variant=ButtonVariant::Primary on_click=|_| {}>
                                        "Créer le compte"
                                    </Button>
                                </form>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}
