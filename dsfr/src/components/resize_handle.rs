use leptos::prelude::*;

/// Poignée de redimensionnement par glissement horizontal (souris) : ajuste
/// `width` (en pixels) en fonction du déplacement depuis le `mousedown`
/// initial sur la poignée. Les écouteurs `mousemove`/`mouseup` sont posés
/// sur `window` (plutôt que sur la poignée elle-même) pour continuer à
/// suivre la souris même si elle quitte la poignée pendant le glissement.
///
/// `width` est censé piloter la largeur (en pixels, via `style:width`) du
/// panneau voisin de cette poignée dans le flux ; un déplacement vers la
/// droite réduit `width`, vers la gauche l'augmente (poignée à gauche d'un
/// panneau ancré à droite).
#[component]
pub fn ResizeHandle(
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
