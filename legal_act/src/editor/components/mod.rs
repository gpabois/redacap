use leptos::{prelude::*, IntoView, context::provide_context, view};
use dsfr::{
    BlocMarianneInline, Button as DsfrButton, ButtonGroup as DsfrButtonGroup, ButtonVariant, Header, ResizeHandle, Size, SubHeader, TabPanel, Tabs,
};
use shared::enclose;
use web_sys::wasm_bindgen::JsCast;
use crate::{
    editor::{
        hooks::use_editor_state, state::TextSignal, tools::{Tool, ToolGroup}
    }, id::NodeId, model::LegalActProject, str_ext::StrExt
};

use super::EditorState;


#[component]
pub fn Editor(act: LegalActProject) -> impl IntoView {
    let state = EditorState::new(act);
    provide_context(state.clone());

    let panel_width = RwSignal::new(10.0);
    let selected_tab = RwSignal::new(0);
    
    view! {
        <div class="legal-act-editor flex flex-col min-h-screen max-h-screen text-base leading-relaxed">
            <div class="no-print">
                <EditorHeader />
            </div>
            <div class="flex flex-1 overflow-hidden">
                <Page>
                    <ActHeader/>
                    <Title/>
                    <Visas/>
                    <Considerants/>
                    <SurRoot/>
                    <h1 class="uppercase my-8 text-2xl text-center">Arrête</h1>
                    <Body/>
                </Page>
                <div class="no-print contents">
                    <ResizeHandle
                        width=panel_width
                        min_width=10.0
                        max_width=60.0
                    />
                    <aside class="shrink-0 overflow-hidden flex flex-col min-h-0" style:width=move || format!("{}%", panel_width.get())>
                        <Tabs titles=vec!["Marie", "Commentaires", "Dev"] selected=selected_tab/>
                        <TabPanel index=0 selected=selected_tab class="flex-1 flex flex-col min-h-0">
                            <div></div>
                        </TabPanel>
                        <TabPanel index=1 selected=selected_tab class="flex-1 flex flex-col min-h-0">
                            <div></div>
                        </TabPanel>
                        <TabPanel index=2 selected=selected_tab class="flex-1 flex flex-col min-h-0">
                            <div>
                                {   
                                    enclose!{(state) move || {
                                        if let Some(cursor) = state.cursor() {
                                            view! {
                                                <table>
                                                    <tr>
                                                        <td>"Node id"</td>
                                                        <td>{cursor.id.to_string()}</td>
                                                    </tr>
                                                    <tr>
                                                        <td>"Offset"</td>
                                                        <td>{cursor.pos}</td>
                                                    </tr>
                                                </table>
                                            }.into_any()
                                        } else {
                                            view!{"Pas de curseur"}.into_any()
                                        }
                                    }}
                                }
                            </div>
                        </TabPanel>
                    </aside>
                </div>
            </div>
        </div>
    }
}   

#[component]
pub fn Page(children: Children) -> impl IntoView {
    view! {
        <main class="flex-1 overflow-y-auto bg-gray-200 dark:bg-gray-800 print:bg-white print:overflow-visible">
            // Le papier reste blanc en mode sombre (métaphore de la
            // feuille imprimée) : seul le plateau qui l'entoure
            // bascule en sombre. `text-gray-900` fixe la couleur du
            // texte indépendamment du thème : sans cela, il hérite
            // de `dark:text-gray-100` posé sur `<main>` (voir
            // `app::app::App`) et devient illisible sur fond blanc.
            <div class="legal-act-page my-4 mx-auto bg-white text-gray-900">
                {children()}
            </div>
        </main>
    }
}

#[component]
pub fn EditorHeader() -> impl IntoView {
    let state = use_editor_state();

    view! {
        <Header service_title="Redac'Ap" service_tagline="Éditeur d'arrêté préfectoral".to_string()>
            <SubHeader slot>
                <For 
                    each=enclose!{(state) move || state.toolbar()}
                    key=|(id, _)| id.clone()
                    children=__component_render_button_group
                />
            </SubHeader>
        </Header>
    }
}

#[component]
pub fn RenderButtonGroup(args: (String, ToolGroup)) -> impl IntoView {
    let (_, group) = args;
    view! {
        <DsfrButtonGroup class="gap-1">
            {group.buttons.into_iter().map(render_button).collect_view()}
        </DsfrButtonGroup>
    }
}

fn render_button(button: Tool) -> impl IntoView {
    let (label, class) = match &button {
        Tool::Bold => ("G", "font-bold"),
        Tool::Italic => ("I", "italic"),
        Tool::Paragraph => ("¶", ""),
        Tool::AppendArticle(_) => ("+ Article", ""),
    };

    view! {
        <DsfrButton
            variant=ButtonVariant::TertiaryNoOutline
            size=Size::Sm
            class=class
            on_click=move |_| {}
        >
            {label}
        </DsfrButton>
    }
}

#[component]
pub fn ActHeader() -> impl IntoView {

    view! {
        <div class="flex">
            <BlocMarianneInline autorite={"Préfet\nde Seine-Maritime"} class="mb-8 flex-1"/>
            <div class="flex-1 mt-[1.1rem] font-size-[1.05rem] font-bold text-right">
                "Direction régionale de l'environnement, de l'aménagement et du logement"
            </div>
        </div>
    }
}

#[component]
pub fn Title() -> impl IntoView {
    let state = use_editor_state();
    let title = state.title();

    view! {
        <div class="flex">
            <InlineTextEdit 
                text={title} 
                class="
                    text-wrap 
                    text-center 
                    text-4xl 
                    font-bold 
                    my-8 
                    flex-1
                " 
            />
        </div>
    }
}

#[component]
pub fn Visas() -> impl IntoView {
    let state = use_editor_state();

    view! {
        <ul data-node-id="visas">
            <For
                each=move || state.visas().children()
                key=|id| id.clone()
                children=|id| view! { <Visa id={id}/> }
            />
        </ul>
    }
}

#[component]
pub fn Visa(id: NodeId) -> impl IntoView {
    let opts = EditOptions {
        plain: true,
        span: true,
        ..Default::default()
    };

    view! {
        <li class="flex">
            <span class="uppercase flex-0 pr-2 font-bold">Vu</span> 
            <ContentEdit root={id} options={opts} />
        </li>
    }
}

#[component]
pub fn Considerants() -> impl IntoView {
    let state = use_editor_state();

    view! {
        <ul data-node-id="considerants">
            <For
                each=move || state.considerants().children()
                key=|id| id.clone()
                children=|id| view! { <Considerant id={id}/> }
            />
        </ul>
    }
}

#[component]
pub fn Considerant(id: NodeId) -> impl IntoView {
    let _state = use_editor_state();
    let opts = EditOptions {
        plain: true,
        span: true,
        ..Default::default()
    };

    view! {
        <li class="flex">
            <span class="uppercase flex-0 pr-2 font-bold">Considérant</span> 
            <ContentEdit root={id} options={opts} />
        </li>
    }
}

#[component]
pub fn SurRoot() -> impl IntoView {
    let state = use_editor_state();

    view! {
        <ul data-node-id="sur">
            <For
                each=move || state.sur().children()
                key=|id| id.clone()
                children=|id| view! { <Visa id={id}/> }
            />
        </ul>
    }
}

#[component]
pub fn Sur(id: NodeId) -> impl IntoView {
    let opts = EditOptions {
        plain: true,
        span: true,
        ..Default::default()
    };

    view! {
        <li class="flex">
            <span class="uppercase flex-0 pr-2 font-bold">Sur</span> 
            <ContentEdit root={id} options={opts} />
        </li>
    }
}

#[component]
pub fn Body() -> impl IntoView {
    let state = use_editor_state();
    let body = state.body();

    __component_render_children(body.get().id())
}

#[component]
pub fn InlineTextEdit(
    text: TextSignal,
    #[prop(into, optional)]
    class: String
) -> impl IntoView {

    let edit = RwSignal::new(false);
    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();
    let click_offset = RwSignal::new(None::<u32>);

    let txt = text.clone();

    Effect::new(move |_| {
        if edit.get()
            && let Some(el) = textarea_ref.get()
        {
            request_animation_frame(move || {
                let _ = el.focus();
            });
        }
    });

    view! {
        <Show
            when=move || edit.get()
            fallback=enclose!{(class, txt) move || view!{
                <span
                    class={class.clone()}
                    on:click:target=move |ev| {
                        let pos = caret_position_from_point(ev.client_x() as f32, ev.client_y() as f32);
                        click_offset.set(pos.map(|(_, offset)| offset));
                        edit.set(true);
                    }
                >
                    {txt.get()}
                </span>
            }}
        >
            <textarea
                class={format!("field-sizing-content resize-none overflow-hidden p-0 m-0 outline-none border-none {class}")}
                node_ref={textarea_ref}
                on:focus:target=move |ev| {
                    if let Some(offset) = click_offset.get_untracked() {
                        let _ = ev.target().set_selection_range(offset, offset);
                    }
                }
                on:blur:target=move |_| edit.set(false)
                prop:value=text.get()
                on:input:target=enclose!{(txt) move |ev| txt.update(ev.target().value())}
            />
        </Show>
    }
}

#[component]
#[allow(unused_variables)]
pub fn ContentEdit(root: NodeId, options: EditOptions) -> impl IntoView {
    let state = use_editor_state();
    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();
    provide_context(textarea_ref);

    view!{
        <div data-node-id={root.clone().to_string()} >
            <textarea
                class="h-0 w-0"
                node_ref={textarea_ref}
                on:input:target=enclose!{(state) move |ev| {
                    let target = ev.target();
                    let inserted = target.value();
                    target.set_value("");
                    if !inserted.is_empty() {
                        state.insert_at_cursor(&inserted);
                    }
                }}
                on:keydown:target=enclose!{(state) move |ev| {
                    match ev.key().as_str() {
                        "Backspace" => {
                            ev.prevent_default();
                            state.delete_backward();
                        },
                        "Delete" => {
                            ev.prevent_default();
                            state.delete_forward();
                        },
                        "ArrowLeft" => {
                            ev.prevent_default();
                            state.backward_cursor();
                        },
                        "ArrowRight" => {
                            ev.prevent_default();
                            state.forward_cursor();
                        },
                        _ => {}
                    }
                }}
            ></textarea>
            <RenderChildren parent={root.clone()}/>
        </div>
    }
}

#[component]
pub fn RenderChildren(parent: NodeId) -> impl IntoView {
    let state = use_editor_state();

    view! {
        <For
            each=move || state.try_node(&parent).unwrap().children()
            key=|id| id.clone()
            children=|id| view! { <RenderContent id={id}/> }
        />
    }
}

#[component]
pub fn Caret() -> impl IntoView {
    view! {<span data-cursor="true" class="animate-blink p-0 m-0">|</span>}
}

#[component]
pub fn Selected(children: Children) -> impl IntoView {
    view!{<span>{children()}</span>}
}

#[component]
pub fn RenderContent(id: NodeId) -> AnyView {
    let state = use_editor_state();
    let node = state.try_node(&id).unwrap();
    
    match node.kind() {
        crate::data::NodeKind::Paragraphe => view! {
            <p>
                <RenderChildren parent={id} />
            </p>
        }.into_any(),
        crate::data::NodeKind::Plain => {
            // Créée une seule fois, dans la portée stable de RenderContent :
            // le RwSignal sous-jacent doit être rattaché à un owner qui vit
            // aussi longtemps que le nœud, faute de quoi le recréer à chaque
            // exécution de la closure réactive ci-dessous le lierait à un
            // owner recréé (donc disposé) à chaque passage, provoquant un
            // panic "already disposed" au second rendu (ex : au clic).
            let text_signal = node.text();
            let textarea_ref = use_context::<NodeRef<leptos::html::Textarea>>();

            view! {
                <span on:click:target=enclose!{(state, id, text_signal, textarea_ref) move |ev| {
                    let char_len = text_signal.get().chars().count();
                    update_cursor(&state, id.clone(), char_len, &ev);
                    if let Some(el) = textarea_ref.and_then(|r| r.get_untracked()) {
                        let _ = el.focus();
                    }
                }}>
                    {enclose!{(state, id, text_signal) move || {
                        let text = text_signal.get();
                        let char_len = text.chars().count();

                        let cursor = state.cursor.get().filter(|c| c.within(&id)).map(|c| c.pos);

                        let selected = state.selection.get().and_then(|span| {
                            match (span.start.id == id, span.end.id == id) {
                                (true, true) => Some((span.start.pos, span.end.pos)),
                                (true, false) => Some((span.start.pos, char_len)),
                                (false, true) => Some((0, span.end.pos)),
                                (false, false) => span.covers(state.act(), &id).then_some((0, char_len)),
                            }
                        });

                        plain_view(&text, cursor, selected)
                    }}}
                </span>
            }.into_any()
        },
        crate::data::NodeKind::Span => {
            let span = node.get().data().as_span().unwrap();
            view! {
                <span 
                    class:font-bold={span.bold}
                    class:underline={span.underline}
                    class:line-through={span.strikeout}
                    class:italic={span.italic}
                >
                    <RenderChildren parent={id.clone()}/>
                </span>
            }.into_any()
        },
        crate::data::NodeKind::Table => {
            view!{
                <table>
                    <RenderChildren parent={id.clone()}/>
                </table>
            }.into_any()
        },
        crate::data::NodeKind::TableRow => {
            view!{
                <tr>
                    <RenderChildren parent={id.clone()}/>
                </tr>
            }.into_any()
        },
        crate::data::NodeKind::TableCell => {
            view!{
                <td>
                    <RenderChildren parent={id.clone()}/>
                </td>
            }.into_any()
        },
        crate::data::NodeKind::List => {
            todo!()
        },
        crate::data::NodeKind::ListItem => {
            todo!()
        },
        _ => ().into_any()
    }
}

/// Place le curseur de [`EditorState`] au caractère du nœud `Plain` `id` le
/// plus proche du point cliqué (`ev`), en résolvant la position DOM du clic
/// via [`caret_position_from_point`] puis en la ramenant à un offset
/// caractère dans le texte du nœud (le marqueur `<Cursor/>` inséré par
/// [`plain_view`] est ignoré : ce n'est pas du texte).
fn update_cursor(state: &EditorState, id: NodeId, char_len: usize, ev: &web_sys::MouseEvent) {
    let Some(container) = ev.current_target() else { return };
    let container: web_sys::Node = container.unchecked_into();

    let Some((target, offset)) = caret_position_from_point(ev.client_x() as f32, ev.client_y() as f32) else { return };

    let mut count = 0usize;
    let pos = accumulate_char_offset(&container, &target, offset, &mut count)
        .unwrap_or(char_len);

    state.cursor.set(Some(crate::selection::Cursor { id, pos }));
}

/// Résout la position DOM (nœud texte + offset UTF-16) sous le point
/// `(x, y)`, en essayant successivement l'API standard W3C
/// `caretPositionFromPoint` (Firefox) puis, si absente, l'API WebKit/Blink
/// `caretRangeFromPoint` (Chrome, Edge, Safari) : `web_sys::Document` ne
/// lie que la première, ce qui laissait le curseur bloqué à `None` sur les
/// navigateurs basés sur Chromium.
fn caret_position_from_point(x: f32, y: f32) -> Option<(web_sys::Node, u32)> {
    use web_sys::wasm_bindgen::JsValue;

    let document = document();
    let doc_val: &JsValue = document.as_ref();
    let x_val = JsValue::from_f64(x as f64);
    let y_val = JsValue::from_f64(y as f64);

    if let Ok(caret_pos_fn_val) = js_sys::Reflect::get(doc_val, &JsValue::from_str("caretPositionFromPoint"))
        && caret_pos_fn_val.is_function()
    {
        let caret_pos_fn: js_sys::Function = caret_pos_fn_val.unchecked_into();
        if let Ok(pos_obj) = caret_pos_fn.call2(doc_val, &x_val, &y_val)
            && !pos_obj.is_null()
            && !pos_obj.is_undefined()
        {
            let offset_node = js_sys::Reflect::get(&pos_obj, &JsValue::from_str("offsetNode")).ok()?;
            let offset = js_sys::Reflect::get(&pos_obj, &JsValue::from_str("offset")).ok()?;
            let node: web_sys::Node = offset_node.dyn_into().ok()?;
            return Some((node, offset.as_f64()? as u32));
        }
    }

    if let Ok(caret_range_fn_val) = js_sys::Reflect::get(doc_val, &JsValue::from_str("caretRangeFromPoint"))
        && caret_range_fn_val.is_function()
    {
        let caret_range_fn: js_sys::Function = caret_range_fn_val.unchecked_into();
        if let Ok(range_obj) = caret_range_fn.call2(doc_val, &x_val, &y_val)
            && !range_obj.is_null()
            && !range_obj.is_undefined()
        {
            let start_container = js_sys::Reflect::get(&range_obj, &JsValue::from_str("startContainer")).ok()?;
            let start_offset = js_sys::Reflect::get(&range_obj, &JsValue::from_str("startOffset")).ok()?;
            let node: web_sys::Node = start_container.dyn_into().ok()?;
            return Some((node, start_offset.as_f64()? as u32));
        }
    }

    None
}

/// Parcourt `node` en profondeur et accumule dans `count` le nombre de
/// caractères des nœuds texte rencontrés (en ignorant le sous-arbre du
/// marqueur `data-cursor`), jusqu'à atteindre `target` (à `target_offset`
/// UTF-16 près). Retourne `Some(position)` une fois `target` trouvé,
/// `None` sinon (et `count` contient alors le total du sous-arbre).
fn accumulate_char_offset(
    node: &web_sys::Node,
    target: &web_sys::Node,
    target_offset: u32,
    count: &mut usize,
) -> Option<usize> {
    if let Some(el) = node.dyn_ref::<web_sys::Element>()
        && el.has_attribute("data-cursor")
    {
        return node.contains(Some(target)).then_some(*count);
    }

    if node.node_type() == web_sys::Node::TEXT_NODE {
        let text = node.text_content().unwrap_or_default();
        if node.is_same_node(Some(target)) {
            return Some(*count + utf16_offset_to_char_offset(&text, target_offset as usize));
        }
        *count += text.chars().count();
        return None;
    }

    let mut child = node.first_child();
    while let Some(c) = child {
        if let Some(found) = accumulate_char_offset(&c, target, target_offset, count) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

fn utf16_offset_to_char_offset(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0usize;
    for (char_idx, ch) in text.chars().enumerate() {
        if units >= utf16_offset {
            return char_idx;
        }
        units += ch.len_utf16();
    }
    text.chars().count()
}

/// Rend le texte d'un `Plain` en isolant la portion sélectionnée dans
/// `<Selected>` et en insérant `<Cursor/>` à la position du curseur.
///
/// Les positions de coupe (curseur, début et fin de la sélection locale à ce
/// nœud) sont résolues via [`StrExt::split_at_positions`].
fn plain_view(text: &str, cursor: Option<usize>, selected: Option<(usize, usize)>) -> AnyView {
    let char_len = text.chars().count();

    let mut positions: Vec<usize> = [cursor, selected.map(|(start, _)| start), selected.map(|(_, end)| end)]
        .into_iter()
        .flatten()
        .collect();
    positions.sort_unstable();
    positions.dedup();

    if positions.is_empty() {
        return text.to_string().into_any();
    }

    let parts = text.split_at_positions(&positions);

    let mut boundaries = Vec::with_capacity(positions.len() + 2);
    boundaries.push(0);
    boundaries.extend_from_slice(&positions);
    boundaries.push(char_len);

    let mut views = Vec::with_capacity(parts.len() * 2);

    // `boundaries` peut contenir une valeur en double (ex : curseur en fin
    // de nœud, où sa position coïncide avec `char_len` déjà présent), ce qui
    // produit un segment de largeur nulle partageant le même `seg_start`
    // qu'un segment précédent. Sans garde, les deux itérations matcheraient
    // `cursor == Some(seg_start)` et dupliqueraient le `<Caret/>`.
    let mut caret_emitted_at: Option<usize> = None;

    for (i, part) in parts.into_iter().enumerate() {
        let seg_start = boundaries[i];
        let seg_end = boundaries[i + 1];

        if cursor == Some(seg_start) && caret_emitted_at != Some(seg_start) {
            views.push(view! { <Caret/> }.into_any());
            caret_emitted_at = Some(seg_start);
        }

        let is_selected = seg_start < seg_end
            && selected.is_some_and(|(start, end)| start <= seg_start && seg_end <= end);

        let owned = part.to_string();
        views.push(if is_selected {
            view! { <Selected>{owned}</Selected> }.into_any()
        } else {
            owned.into_any()
        });
    }

    views.into_any()
}

#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct EditOptions {
    paragraph: bool,
    plain: bool,
    span: bool,
    list: bool,
    table: bool
}