use leptos::html;
use leptos::prelude::*;
use web_sys::wasm_bindgen::JsCast;
use web_sys::HtmlDocument;

use crate::BodyNodeId;
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

// ── Utilitaires curseur / sélection ───────────────────────────────────────────

/// Renvoie `true` si le curseur est collapsé tout au début du `div`.
fn cursor_at_document_start(el: &web_sys::HtmlDivElement) -> bool {
    let Ok(Some(sel)) = document().get_selection() else { return false };
    if !sel.is_collapsed() { return false }
    if sel.range_count() == 0 { return false }
    let Ok(range) = sel.get_range_at(0) else { return false };
    if range.start_offset().unwrap_or(1) != 0 { return false }
    let Ok(container) = range.start_container() else { return false };
    let el_node: &web_sys::Node = el.unchecked_ref();
    let mut node = container;
    loop {
        if node.is_same_node(Some(el_node)) { return true }
        if node.previous_sibling().is_some() { return false }
        match node.parent_node() {
            Some(p) => node = p,
            None => return false,
        }
    }
}

/// Renvoie `true` si le curseur est collapsé tout à la fin du `div`.
fn cursor_at_document_end(el: &web_sys::HtmlDivElement) -> bool {
    let Ok(Some(sel)) = document().get_selection() else { return false };
    if !sel.is_collapsed() { return false }
    if sel.range_count() == 0 { return false }
    let Ok(range) = sel.get_range_at(0) else { return false };
    let Ok(container) = range.start_container() else { return false };
    let offset = range.start_offset().unwrap_or(0) as usize;
    let text_len = container.text_content().unwrap_or_default().encode_utf16().count();
    if offset != text_len { return false }
    let el_node: &web_sys::Node = el.unchecked_ref();
    let mut node = container;
    loop {
        if node.is_same_node(Some(el_node)) { return true }
        if node.next_sibling().is_some() { return false }
        match node.parent_node() {
            Some(p) => node = p,
            None => return false,
        }
    }
}

/// Place le curseur à la fin de tout le contenu du `div`.
pub(super) fn set_cursor_to_end(el: &web_sys::HtmlDivElement) {
    let doc = document();
    let Ok(range) = doc.create_range() else { return };
    let el_node: &web_sys::Node = el.unchecked_ref();
    if range.select_node_contents(el_node).is_err() { return }
    range.collapse_with_to_start(false);
    if let Ok(Some(sel)) = doc.get_selection() {
        let _ = sel.remove_all_ranges();
        let _ = sel.add_range(&range);
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
///
/// `focus_node_id` — identifiant du nœud du corps représenté par ce div :
/// au focus, il est poussé dans [`super::context::EditorContext::content_focus_node`]
/// pour que la barre de contenu contextuelle puisse l'afficher.
///
/// `on_enter` — touche Entrée : sauvegarde le contenu puis déclenche le callback.
/// `on_backspace_start` — Backspace avec le curseur au tout début du div.
/// `on_delete_end` — Delete avec le curseur à la toute fin du div.
///
/// Le focus programmatique passe par
/// [`super::context::EditorContext::content_focus_request`] : quand ce signal
/// contient `Some((focus_node_id, at_end))`, le div se met en focus et, si
/// `at_end`, place le curseur à la fin.
#[component]
pub(super) fn RichEditableDiv(
    #[prop(into)]
    html: Signal<String>,
    on_save: impl Fn(String) + Clone + Send + Sync + 'static,
    #[prop(optional)]
    class: &'static str,
    /// Nœud du corps représenté par ce div (pour le suivi du focus contextuel).
    #[prop(optional)]
    focus_node_id: Option<BodyNodeId>,
    /// Callback appelé quand Entrée est pressée (annule l'action navigateur).
    #[prop(optional)]
    on_enter: Option<Callback<()>>,
    /// Callback appelé quand Backspace est pressé et le curseur est au début.
    #[prop(optional)]
    on_backspace_start: Option<Callback<()>>,
    /// Callback appelé quand Delete est pressé et le curseur est à la fin.
    #[prop(optional)]
    on_delete_end: Option<Callback<()>>,
) -> impl IntoView {
    let ctx = expect_editor_context();
    let div_ref = NodeRef::<html::Div>::new();
    let is_focused = RwSignal::new(false);

    let on_save_blur = on_save.clone();
    let on_save_key = on_save;

    // Synchronise le signal → DOM hors focus.
    Effect::new(move |_| {
        if let Some(el) = div_ref.get() {
            let h = html.get();
            if !is_focused.get_untracked() && el.inner_html() != h {
                el.set_inner_html(&h);
            }
        }
    });

    // Focus programmatique via content_focus_request.
    Effect::new(move |_| {
        if let Some((req_id, at_end)) = ctx.content_focus_request.get() {
            if Some(req_id) == focus_node_id {
                if let Some(el) = div_ref.get() {
                    let _ = el.focus();
                    if at_end { set_cursor_to_end(&el); }
                    ctx.content_focus_request.set(None);
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
                 focus:border-teal-400 hover:border-gray-300 \
                 transition-colors cursor-text {class}"
            )
            on:focus=move |_| {
                is_focused.set(true);
                ctx.content_focus.set(true);
                if let Some(id) = focus_node_id {
                    ctx.content_focus_node.set(Some(id));
                }
            }
            on:blur=move |_| {
                is_focused.set(false);
                ctx.content_focus.set(false);
                if focus_node_id.map(|id| ctx.content_focus_node.get_untracked() == Some(id)).unwrap_or(false) {
                    ctx.content_focus_node.set(None);
                }
                if let Some(el) = div_ref.get() {
                    on_save_blur(el.inner_html());
                }
            }
            on:keydown=move |ev| {
                let Some(el) = div_ref.get() else { return };
                match ev.key().as_str() {
                    "Enter" => {
                        if let Some(cb) = on_enter {
                            ev.prevent_default();
                            on_save_key.clone()(el.inner_html());
                            cb.run(());
                        }
                    }
                    "Backspace" => {
                        if let Some(cb) = on_backspace_start {
                            if cursor_at_document_start(&el) {
                                ev.prevent_default();
                                on_save_key.clone()(el.inner_html());
                                cb.run(());
                            }
                        }
                    }
                    "Delete" => {
                        if let Some(cb) = on_delete_end {
                            if cursor_at_document_end(&el) {
                                ev.prevent_default();
                                on_save_key.clone()(el.inner_html());
                                cb.run(());
                                // Forcer la mise à jour du div (on reste en focus)
                                let new_html = html.get_untracked();
                                if el.inner_html() != new_html {
                                    el.set_inner_html(&new_html);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        />
    }
}
