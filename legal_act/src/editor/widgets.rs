use leptos::html;
use leptos::prelude::*;
use web_sys::wasm_bindgen::JsCast;
use web_sys::HtmlDocument;

pub(super) const TOOLBAR_BTN_CLASS: &str =
    "text-xs border border-teal-600 text-teal-600 rounded px-2 py-0.5 \
     hover:bg-teal-50 cursor-pointer";

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

/// Barre de formatage inline (G/I/S/B). Les boutons utilisent `mousedown` +
/// `prevent_default` pour ne pas interrompre le focus du div contenteditable.
#[component]
fn ContentFormatToolbar() -> impl IntoView {
    let btn = "w-6 h-6 flex items-center justify-center hover:bg-gray-100 \
               rounded cursor-pointer select-none";
    view! {
        <div class="flex gap-0.5 mb-0.5 px-1 py-0.5 bg-white border border-gray-200 \
                    rounded shadow-sm text-xs select-none">
            <button type="button" title="Gras" class=format!("{btn} font-bold")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("bold"); }>
                "G"
            </button>
            <button type="button" title="Italique" class=format!("{btn} italic")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("italic"); }>
                "I"
            </button>
            <button type="button" title="Souligné" class=format!("{btn} underline")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("underline"); }>
                "S"
            </button>
            <button type="button" title="Barré" class=format!("{btn} line-through")
                on:mousedown=|ev| { ev.prevent_default(); exec_format_command("strikeThrough"); }>
                "B"
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
        <div class="rich-editor-wrapper">
            <Show when=move || is_focused.get()>
                <ContentFormatToolbar/>
            </Show>
            <div
                node_ref=div_ref
                contenteditable="true"
                class=format!(
                    "outline-none min-w-[4ch] border-b border-transparent \
                     focus:border-teal-400 hover:border-gray-300 \
                     transition-colors cursor-text {class}"
                )
                on:focus=move |_| is_focused.set(true)
                on:blur=move |_| {
                    is_focused.set(false);
                    if let Some(el) = div_ref.get() {
                        on_save(el.inner_html());
                    }
                }
            />
        </div>
    }
}
