use leptos::html;
use leptos::prelude::*;
use web_sys::wasm_bindgen::JsCast;
use web_sys::HtmlDocument;

use super::context::expect_editor_context;

pub(super) const TOOLBAR_BTN_CLASS: &str =
    "no-print text-xs border border-teal-600 text-teal-600 rounded px-2 py-0.5 \
     hover:bg-teal-50 cursor-pointer";

/// Poignée de redimensionnement par glissement horizontal (souris) : ajuste
/// `width` (en pixels) en fonction du déplacement depuis le `mousedown`
/// initial sur la poignée. Les écouteurs `mousemove`/`mouseup` sont posés
/// sur `window` (plutôt que sur la poignée elle-même) pour continuer à
/// suivre la souris même si elle quitte la poignée pendant le glissement —
/// même idiome que `content::editor::components::EditRoot` pour le suivi de
/// sélection.
///
/// `width` est censé piloter la largeur (en pixels, via `style:width`) du
/// panneau voisin de cette poignée dans le flux ; un déplacement vers la
/// droite réduit `width`, vers la gauche l'augmente (poignée à gauche d'un
/// panneau ancré à droite).
#[component]
pub(super) fn ResizeHandle(
    /// Largeur ajustée par le glissement (en pixels).
    width: RwSignal<f64>,
    /// Largeur minimale autorisée (en pixels).
    #[prop(default = 0.0)]
    min_width: f64,
    /// Largeur maximale autorisée (en pixels).
    #[prop(default = f64::MAX)]
    max_width: f64,
) -> impl IntoView {
    // `Some((abscisse du mousedown, largeur au mousedown))` pendant le
    // glissement, `None` sinon.
    let drag_origin = RwSignal::<Option<(f64, f64)>>::new(None);

    window_event_listener(leptos::ev::mousemove, move |ev| {
        if let Some((origin_x, origin_width)) = drag_origin.get_untracked() {
            let delta = f64::from(ev.client_x()) - origin_x;
            width.set((origin_width - delta).clamp(min_width, max_width));
        }
    });

    window_event_listener(leptos::ev::mouseup, move |_| {
        drag_origin.set(None);
    });

    view! {
        <div
            class="w-1 shrink-0 cursor-col-resize select-none bg-gray-300 \
                   hover:bg-blue-france transition-colors"
            class:bg-blue-france=move || drag_origin.get().is_some()
            on:mousedown=move |ev| {
                ev.prevent_default();
                drag_origin.set(Some((f64::from(ev.client_x()), width.get_untracked())));
            }
        ></div>
    }
}

/// `<div contenteditable>` réactif : synchronise le signal → DOM hors focus,
/// et appelle `on_save` au `blur`.
#[component]
pub(super) fn InlineEditableDiv(
    /// Texte courant (source de vérité).
    #[prop(into)]
    text: Signal<String>,
    /// Appelé au `blur` avec le contenu textuel du div.
    on_save: impl Fn(String) + Clone + Send + Sync + 'static,
    /// Classes Tailwind supplémentaires.
    #[prop(optional)]
    class: &'static str,
    /// Bloque la touche Entrée (utile pour les libellés sur une ligne).
    #[prop(default = false)]
    prevent_newlines: bool,
) -> impl IntoView {
    let div_ref = NodeRef::<html::Div>::new();
    let is_focused = RwSignal::new(false);
    let initial = text.get_untracked();

    // Sync signal → DOM uniquement quand le div n'est pas en focus.
    Effect::new(move |_| {
        let t = text.get();
        if !is_focused.get_untracked() {
            if let Some(el) = div_ref.get() {
                if el.inner_text() != t {
                    el.set_inner_text(&t);
                }
            }
        }
    });

    view! {
        <div
            node_ref=div_ref
            contenteditable="true"
            class=format!(
                "outline-none min-w-[4ch] border-b border-transparent \
                 focus:border-blue-france hover:border-gray-300 \
                 transition-colors cursor-text {class}"
            )
            on:focus=move |_| is_focused.set(true)
            on:blur=move |_| {
                is_focused.set(false);
                if let Some(el) = div_ref.get() {
                    on_save(el.inner_text());
                }
            }
            on:keydown=move |ev| {
                if prevent_newlines && ev.key() == "Enter" {
                    ev.prevent_default();
                }
            }
        >
            {initial}
        </div>
    }
}

// ── Barre de formatage ────────────────────────────────────────────────────────

fn exec_format_command(cmd: &str) {
    let doc = document();
    if let Some(html_doc) = doc.dyn_ref::<HtmlDocument>() {
        let _ = html_doc.exec_command(cmd);
    }
}

/// Barre de formatage inline (B/G/I), affichée dans le sous-en-tête (voir
/// [`super::header::EditorHeader`]) tant qu'un [`RichEditableDiv`] a le
/// focus (voir [`super::context::EditorContext::content_focus`]). Les
/// boutons utilisent `mousedown` + `prevent_default` pour ne pas interrompre
/// le focus du div contenteditable (nécessaire pour que `exec_command`
/// s'applique à la sélection en cours).
#[component]
pub(super) fn FormatToolbar() -> impl IntoView {
    let btn = "w-6 h-6 flex items-center justify-center hover:bg-gray-100 \
               rounded cursor-pointer select-none";
    view! {
        <div class="flex gap-0.5 text-xs select-none">
            <button type="button" title="Barré" class=format!("{btn} line-through")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("strikeThrough"); }>
                "B"
            </button>
            <button type="button" title="Gras" class=format!("{btn} font-bold")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("bold"); }>
                "G"
            </button>
            <button type="button" title="Italique" class=format!("{btn} italic")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("italic"); }>
                "I"
            </button>
        </div>
    }
}

// ── RichEditableDiv ───────────────────────────────────────────────────────────

/// Div `contenteditable` pour le texte riche (HTML inline : `<strong>`, `<em>`,
/// `<u>`, `<s>`). Affiche la barre de formatage lorsqu'il est en focus.
/// Synchronise signal → DOM hors focus ; appelle `on_save` avec l'`innerHTML`
/// au `blur`.
#[component]
pub(super) fn RichEditableDiv(
    #[prop(into)]
    html: Signal<String>,
    on_save: impl Fn(String) + Clone + Send + Sync + 'static,
    #[prop(optional)]
    class: &'static str,
) -> impl IntoView {
    let ctx = expect_editor_context();
    let div_ref = NodeRef::<html::Div>::new();
    let is_focused = RwSignal::new(false);

    // Synchronise le signal → DOM chaque fois que html change et que l'élément
    // n'est pas en cours d'édition. Fonctionne aussi au premier montage grâce
    // au tracking de `div_ref`.
    Effect::new(move |_| {
        if let Some(el) = div_ref.get() {
            let h = html.get();
            if !is_focused.get_untracked() && el.inner_html() != h {
                el.set_inner_html(&h);
            }
        }
    });

    view! {
        <div
            node_ref=div_ref
            contenteditable="true"
            class=format!(
                "outline-none min-w-[4ch] border-b border-transparent \
                 focus:border-teal-400 hover:border-gray-300 \
                 transition-colors cursor-text {class}"
            )
            on:focus=move |_| {
                is_focused.set(true);
                ctx.content_focus.set(true);
            }
            on:blur=move |_| {
                is_focused.set(false);
                ctx.content_focus.set(false);
                if let Some(el) = div_ref.get() {
                    on_save(el.inner_html());
                }
            }
        />
    }
}
