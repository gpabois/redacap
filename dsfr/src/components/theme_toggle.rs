//! Bascule de thème clair / sombre / système, affichée dans la zone outils
//! de [`super::Header`].
//!
//! Le thème est appliqué en ajoutant/retirant la classe `dark` sur
//! `<html>` (voir `@custom-variant dark` dans `style/input.css`) plutôt que
//! par la seule `prefers-color-scheme` : cela permet à l'utilisateur de
//! forcer un thème indépendamment du système d'exploitation. La préférence
//! est persistée en `localStorage` pour survivre à un rechargement.

use leptos::prelude::*;

/// Script à injecter tel quel, tôt dans `<head>` (avant `<HydrationScripts>`,
/// voir `app::app::shell`), pour poser la classe `dark` avant le premier
/// rendu et éviter un flash du mauvais thème : le WASM n'est pas encore
/// chargé à ce stade, d'où ce script inline en JavaScript brut.
pub const THEME_INIT_SCRIPT: &str = r#"(function(){try{var m=localStorage.getItem('redacap-theme');var d=m==='dark'||(m!=='light'&&window.matchMedia('(prefers-color-scheme: dark)').matches);document.documentElement.classList.toggle('dark',d);}catch(e){}})();"#;

/// Préférence de thème choisie par l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    /// Mode suivant dans le cycle clair → sombre → système → clair…
    fn next(self) -> Self {
        match self {
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::System,
            ThemeMode::System => ThemeMode::Light,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "Thème clair (cliquer pour le thème sombre)",
            ThemeMode::Dark => "Thème sombre (cliquer pour suivre le système)",
            ThemeMode::System => "Thème système (cliquer pour le thème clair)",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            ThemeMode::Light => "☀",
            ThemeMode::Dark => "☾",
            ThemeMode::System => "◐",
        }
    }
}

/// Bouton de bascule de thème, prêt à l'emploi : gère lui-même sa
/// persistance et l'application de la classe `dark`.
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let mode = RwSignal::new(browser::initial_mode());

    Effect::new(move |_| {
        browser::apply(mode.get());
    });

    view! {
        <button
            type="button"
            title=move || mode.get().label()
            aria-label=move || mode.get().label()
            on:click=move |_| mode.update(|current| *current = current.next())
            class="flex items-center justify-center w-9 h-9 rounded-full text-lg leading-none text-blue-france hover:bg-blue-france-975 dark:text-gray-200 dark:hover:bg-gray-800 transition-colors shrink-0 cursor-pointer"
        >
            {move || mode.get().icon()}
        </button>
    }
}

#[cfg(not(feature = "ssr"))]
mod browser {
    use super::ThemeMode;
    use leptos::prelude::window;
    use web_sys::wasm_bindgen::JsCast;
    use web_sys::wasm_bindgen::closure::Closure;

    /// Clé `localStorage` sous laquelle la préférence est persistée. Doit
    /// rester synchronisée avec la clé écrite en dur dans
    /// [`super::THEME_INIT_SCRIPT`].
    const STORAGE_KEY: &str = "redacap-theme";

    fn mode_as_str(mode: ThemeMode) -> &'static str {
        match mode {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::System => "system",
        }
    }

    fn mode_parse(value: &str) -> ThemeMode {
        match value {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }

    pub fn initial_mode() -> ThemeMode {
        window()
            .local_storage()
            .ok()
            .flatten()
            .and_then(|storage| storage.get_item(STORAGE_KEY).ok().flatten())
            .map(|value| mode_parse(&value))
            .unwrap_or_default()
    }

    fn prefers_dark() -> bool {
        window()
            .match_media("(prefers-color-scheme: dark)")
            .ok()
            .flatten()
            .is_some_and(|query| query.matches())
    }

    fn set_dark_class(dark: bool) {
        let Some(document_element) = window().document().and_then(|doc| doc.document_element())
        else {
            return;
        };
        let class_list = document_element.class_list();
        let _ = if dark {
            class_list.add_1("dark")
        } else {
            class_list.remove_1("dark")
        };
    }

    /// Applique `mode`, le persiste, et — en mode [`ThemeMode::System`] —
    /// s'abonne aux changements de préférence de l'OS pour la durée de vie
    /// de la page (le `Closure` est volontairement "oublié", comme pour le
    /// websocket de collaboration dans `app::ws::open_socket`).
    pub fn apply(mode: ThemeMode) {
        if let Ok(Some(storage)) = window().local_storage() {
            let _ = storage.set_item(STORAGE_KEY, mode_as_str(mode));
        }

        let dark = match mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => prefers_dark(),
        };
        set_dark_class(dark);

        if mode == ThemeMode::System
            && let Ok(Some(query)) = window().match_media("(prefers-color-scheme: dark)")
        {
            let closure = Closure::<dyn FnMut()>::new(move || {
                set_dark_class(prefers_dark());
            });
            query.set_onchange(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
    }
}

#[cfg(feature = "ssr")]
mod browser {
    use super::ThemeMode;

    pub fn initial_mode() -> ThemeMode {
        ThemeMode::default()
    }

    pub fn apply(_mode: ThemeMode) {}
}
