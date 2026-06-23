
use leptos::*;
use leptos::prelude::*;
use web_sys::KeyboardEvent;


#[component]
pub fn InlineEditableField(
    /// Le signal contenant la valeur actuelle (lecture)
    #[prop(into)] value: Signal<String>,
    #[prop(optional)]
    class: &'static str,
    /// Le callback déclenché lorsque la valeur est validée/modifiée
    on_save: impl Fn(String) + Clone + Send + 'static,
) -> impl IntoView {
    let (is_editing, set_is_editing) = signal(false);
    let (temp_value, set_temp_value) = signal(value.get_untracked());

    Effect::new(move |_| {
        set_temp_value.set(value.get());
    });

    let save = move || {
        let new_val = temp_value.get().trim().to_string();
        on_save(new_val);
        set_is_editing.set(false);
    };

    // Style commun partagé pour s'assurer que la taille et l'alignement restent identiques
    
    view! {
        <div class="inline-block">
            {move || if is_editing.get() {
                let save_1 = save.clone();
                let save_2 = save.clone();
                view! {
                    <input
                        type="text"
                        // On applique base_classes, et on ajoute un fond et une bordure de focus non-disruptive
                        class=format!("{class} border-b-2 border-black outline-none w-full")
                        value=temp_value.get()
                        on:input=move |ev| set_temp_value.set(event_target_value(&ev))
                        on:blur=move |_| save_1()
                        on:keydown=move |ev: KeyboardEvent| {
                            if ev.key() == "Enter" {
                                save_2();
                            } else if ev.key() == "Escape" {
                                set_temp_value.set(value.get());
                                set_is_editing.set(false);
                            }
                        }
                        prop:autofocus=true
                    />
                }.into_any()
            } else {
                view! {
                    <div 
                        // On applique base_classes, et on ajoute juste un effet au survol (hover)
                        class=format!("{class} border-b-2 border-transparent cursor-pointer after:content-['✎'] after:ml-1.5 after:text-gray-400 after:text-sm after:opacity-0 after:transition-opacity hover:after:opacity-100")
                        on:click=move |_| set_is_editing.set(true)
                    >
                        {value}
                    </div>
                }.into_any()
            }}
        </div>
    }
}