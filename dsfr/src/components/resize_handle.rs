use leptos::prelude::*;

/// Largeur (px) du plus proche ancêtre doté d'une véritable boîte de mise en
/// page, en sautant les ancêtres `display: contents` (ex. wrappers
/// `no-print contents`) dont `getBoundingClientRect()` renvoie une largeur
/// nulle.
fn nearest_laid_out_ancestor_width(el: &web_sys::Element) -> Option<f64> {
    let mut current = el.parent_element();
    while let Some(ancestor) = current {
        let width = ancestor.get_bounding_client_rect().width();
        if width > 0.0 {
            return Some(width);
        }
        current = ancestor.parent_element();
    }
    None
}

/// Poignée de redimensionnement par glissement horizontal (souris) : ajuste
/// `width` (en pourcentage de la largeur du conteneur parent de la poignée)
/// en fonction du déplacement depuis le `mousedown` initial sur la poignée.
/// Les écouteurs `mousemove`/`mouseup` sont posés sur `window` (plutôt que
/// sur la poignée elle-même) pour continuer à suivre la souris même si elle
/// quitte la poignée pendant le glissement.
///
/// `width` est censé piloter la largeur (en %, via `style:width`) du panneau
/// voisin de cette poignée dans le flux ; un déplacement vers la droite
/// réduit `width`, vers la gauche l'augmente (poignée à gauche d'un panneau
/// ancré à droite). Le déplacement en pixels de la souris est converti en
/// pourcentage à partir de la largeur du conteneur parent de la poignée,
/// mesurée au `mousedown` : `width` reste donc cohérent quelle que soit la
/// largeur de l'écran.
#[component]
pub fn ResizeHandle(
    /// Largeur ajustée par le glissement (en % du conteneur parent).
    width: RwSignal<f64>,
    /// Largeur minimale autorisée (en %).
    #[prop(default = 0.0)]
    min_width: f64,
    /// Largeur maximale autorisée (en %).
    #[prop(default = 100.0)]
    max_width: f64,
) -> impl IntoView {
    let handle_ref = NodeRef::<leptos::html::Div>::new();

    // `Some((abscisse du mousedown, largeur au mousedown, largeur du
    // conteneur parent en px))` pendant le glissement, `None` sinon.
    let drag_origin = RwSignal::<Option<(f64, f64, f64)>>::new(None);

    let mousemove_handle = window_event_listener(leptos::ev::mousemove, move |ev| {
        if let Some((origin_x, origin_width, container_width)) =
            drag_origin.try_get_untracked().flatten()
            && container_width > 0.0
        {
            let delta_px = f64::from(ev.client_x()) - origin_x;
            let delta_percent = delta_px / container_width * 100.0;
            width.set((origin_width - delta_percent).clamp(min_width, max_width));
        }
    });

    let mouseup_handle = window_event_listener(leptos::ev::mouseup, move |_| {
        drag_origin.set(None);
    });

    on_cleanup(move || {
        mousemove_handle.remove();
        mouseup_handle.remove();
    });

    view! {
        <div
            node_ref=handle_ref
            class="w-1 shrink-0 cursor-col-resize select-none bg-gray-300 \
                   hover:bg-blue-france transition-colors"
            class:bg-blue-france=move || drag_origin.get().is_some()
            on:mousedown=move |ev| {
                ev.prevent_default();
                let container_width = handle_ref
                    .get_untracked()
                    .and_then(|el| nearest_laid_out_ancestor_width(&el))
                    .unwrap_or(0.0);
                drag_origin.set(Some((
                    f64::from(ev.client_x()),
                    width.get_untracked(),
                    container_width,
                )));
            }
        ></div>
    }
}
