//! Page `/account` : informations du compte de l'utilisateur courant — voir
//! `Claude.md` § Pages de l'application. Accessible depuis la bulle d'avatar
//! de l'en-tête (voir `crate::pages::dashboard::PageDashboard`).

use dsfr::Alert;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountInfo {
    display_name: String,
    email: String,
}

#[server]
async fn fetch_account_info() -> Result<AccountInfo, ServerFnError> {
    let user_id = match crate::auth::current_user_id().await {
        Ok(user_id) => user_id,
        Err(error) => {
            leptos_axum::redirect("/login");
            return Err(error);
        }
    };

    let pool = expect_context::<storage::Pool>();
    let user = storage::user::get_user(&pool, &user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(AccountInfo {
        display_name: user.display_name,
        email: user.email,
    })
}

#[component]
pub fn PageAccount() -> impl IntoView {
    let account = Resource::new(|| (), |_| fetch_account_info());

    view! {
        <div class="min-h-screen bg-gray-50">
            <div class="max-w-xl mx-auto p-6 flex flex-col gap-6">
                <div>
                    <h1 class="text-xl font-bold text-gray-900">"Mon compte"</h1>
                    <a href="/" class="text-sm text-blue-france hover:underline">"← Retour au tableau de bord"</a>
                </div>

                <Suspense fallback=|| view! { <p class="text-gray-500">"Chargement…"</p> }>
                    {move || Suspend::new(async move {
                        match account.await {
                            Err(error) => view! {
                                <Alert severity=dsfr::components::common::Severity::Error>
                                    {error.to_string()}
                                </Alert>
                            }.into_any(),
                            Ok(account) => view! {
                                <div class="bg-white border border-gray-200 rounded-sm p-6 flex flex-col gap-4">
                                    <div>
                                        <span class="block text-xs font-bold text-gray-500 uppercase">"Nom affiché"</span>
                                        <span class="text-base text-gray-900">{account.display_name}</span>
                                    </div>
                                    <div>
                                        <span class="block text-xs font-bold text-gray-500 uppercase">"Email"</span>
                                        <span class="text-base text-gray-900">{account.email}</span>
                                    </div>
                                </div>
                            }.into_any(),
                        }
                    })}
                </Suspense>

                <a
                    href="/logout"
                    class="self-start bg-transparent text-blue-france shadow-[inset_0_0_0_1px] shadow-gray-300 hover:bg-blue-france-975 font-bold px-4 py-2 transition-colors"
                >
                    "Se déconnecter"
                </a>
            </div>
        </div>
    }
}
