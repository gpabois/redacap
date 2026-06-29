use std::collections::VecDeque;

use leptos::*;
use leptos::{html, prelude::*};
use shared::id::IdGenerator;
use web_sys::{FocusEvent, InputEvent};

use crate::{ContentHandle, ContentKind, ContentRead, Cursor};

use super::context::{
    CurrentContentId, EditorCursor, use_current_children, use_current_content_id, use_current_text, use_editor,
    use_editor_content_body, use_editor_cursor, use_editor_selection,
};
use super::core::Editor;
use super::event::EditorEvent;
use super::selection::{EditorSelection, Selection};

#[component]
pub fn ContentEditor(value: ContentHandle) -> impl IntoView {
    let root = value.root();
    let first_leaf = value.first_leaf_of(root);

    let cursor = EditorCursor {
        id: IdGenerator::default().next_id(),
        caret: Cursor { content_id: first_leaf, offset: 0 },
        mouse: Cursor { content_id: first_leaf, offset: 0 },
        display: false,
    };

    let ev_queue = StoredValue::new(VecDeque::<EditorEvent>::new());

    let cursor = RwSignal::new(cursor);
    let body = RwSignal::new(value);
    let selection = RwSignal::new(EditorSelection::default());

    let editor = Editor { cursor, body, selection, ev_queue };

    provide_context(editor);
    provide_context(CurrentContentId(root));

    view! { <EditNode/> }
}

#[component]
pub fn EditNode() -> impl IntoView {
    use ContentKind::*;

    let body = use_editor_content_body();
    let content_id = use_current_content_id();

    let redirect = move || {
        match body.read().kind_of(content_id) {
            Root => view! {<EditRoot/>}.into_any(),
            Paragraph => view! {<EditParagraph/>}.into_any(),
            Plain => view! {<EditPlain/>}.into_any(),
            Span => todo!(),
            List => todo!(),
            ListItem => todo!(),
            Table => todo!(),
            Row => todo!(),
            Cell => todo!(),
        }
    };

    view! ({redirect})
}

#[component]
pub fn Caret() -> impl IntoView {
    let cursor = use_editor_cursor();
    let id = cursor.get_untracked().id;

    view! {
        <span
            id=format!("caret-{id}")
            class="animate-blink inline-block p-0 text-blue-500 dark:text-white font-normal -mx-[0.15em]">|</span>
    }
}

#[component]
pub fn PlainFragment(children: Children, offset: usize) -> impl IntoView {
    let content_id = use_current_content_id();

    view! {
        <span
            data-content-id={content_id.to_string()}
            data-content-offset={offset.to_string()}
        >{children()}</span>
    }
}

#[component]
pub fn Selected(children: Children, offset: usize) -> impl IntoView {
    let content_id = use_current_content_id();

    view! {
        <span class="bg-stone-800 text-white" id="selected-content"
            data-content-id={content_id.to_string()}
            data-content-offset={offset.to_string()}
        >{children()}</span>
    }
}

#[component]
pub fn EditorDebugData() -> impl IntoView {
    let editor = use_editor();

    let selection = move || {
        let sel = editor.selection.get();

        view ! {
            <div class="flex">
                <div class="flex-auto">{sel.state.as_ref().to_string()}</div>
                <div class="flex-auto">"Anchor": {sel.anchor.map(|cursor| cursor.to_string()).unwrap_or(String::from("-"))}</div>
                <div class="flex-auto">"Focus": {sel.focus.map(|cursor| cursor.to_string()).unwrap_or(String::from("-"))}</div>
            </div>
        }
    };

    view! {
        <div>
            "Selection": {selection}
        </div>
        <div>{move || format!("Mouse: {}", editor.cursor.get().mouse)}</div>
        <div>{move || format!("Caret: {}", editor.cursor.get().caret)}</div>
    }
}

#[component]
pub fn EditRoot() -> impl IntoView {
    let editor = use_editor();
    let children = use_current_children();
    let ref_textarea: NodeRef<html::Textarea> = NodeRef::<html::Textarea>::new();

    let on_before_input = move |e: InputEvent| {
        if let Some(text) = e.data() {
            editor.write_str(text.as_str());
            ref_textarea.get().unwrap().set_value("");
        }
    };

    let on_focus = move |_: FocusEvent| {
        editor.process_event(EditorEvent::Focus);
    };

    let on_focus_lost = move |_| {
        editor.process_event(EditorEvent::Blur);
    };

    let on_mouse_click = move |ev: web_sys::MouseEvent| {
        if let Some(el) = ref_textarea.get() {
            el.focus().unwrap();
        }

        editor.process_event(EditorEvent::MouseClick(ev));
    };

    window_event_listener(leptos::ev::mouseup, move |ev| {
        editor.process_event(EditorEvent::MouseUp(ev));
    });

    view! {
        <EditorDebugData/>

        <div
            on:keydown=move |ev| editor.send_event(EditorEvent::KeyDown(ev))
            on:mousedown=move |ev| editor.send_event(EditorEvent::MouseDown(ev))
            on:mouseenter = move |ev| editor.send_event(EditorEvent::MouseEnter(ev))
            on:mousemove=move |ev| editor.send_event(EditorEvent::MouseMove(ev))
            on:click=on_mouse_click
            on:focus=on_focus
            on:blur=on_focus_lost
            tabindex="0"
            class="
                select-none
                relative
                whitespace-pre-wrap
                focus:outline-none
                before:h-full
                before:w-[4px]
                before:absolute before:top-0 before:left-[-20px]
                before:scale-y-0
                before:transition-transform before:duration-200 before:ease-in-out
                focus-within:before:scale-y-100
                before:rounded-xs
                fr-y
            ">
            <textarea
                node_ref = ref_textarea
                id="hidden-capture"
                class="absolute opacity-0 pointer-events-none w-px h-px"
                on:beforeinput=on_before_input
            ></textarea>
            <For each=children key=|cid| *cid children=|cid| {
                provide_context(CurrentContentId(cid));
                view! {<EditNode/>}
            }/>
        </div>
    }
}

#[component]
pub fn EditParagraph() -> impl IntoView {
    let content_id = use_current_content_id();
    let children = use_current_children();

    view! {
        <p data-content-id={content_id.to_string()} class="text-justify mb-2">
            <For each=children key=|cid| *cid children=|cid| {
                provide_context(CurrentContentId(cid));
                view! {<EditNode/>}
            }/>
        </p>
    }
}

#[component]
pub fn EditPlain() -> impl IntoView {
    let content = use_current_text();
    let content_id = use_current_content_id();
    let cursor = use_editor_cursor();
    let selection = use_editor_selection();
    let body = use_editor_content_body();

    let render = move || {
        let content = content();
        let mut caret = cursor.get().caret;
        let selection = selection.read();

        view! {
            <span data-content-id={content_id.to_string()} data-content-offset=0.to_string()>
            {
                if caret.is_content_within(content_id) {
                    match selection.is_plain_selected(content_id, &body.read_untracked()) {
                        Selection::Nothing => {
                            let (before_caret, after_caret) = caret.split_clone(&content);
                            view! {
                                {before_caret}
                                <Caret/>
                                {after_caret}
                            }.into_any()
                        },
                        Selection::Span(anchor, focus) => {
                            let mut anchor_cursor = caret;
                            anchor_cursor.offset = anchor;

                            let mut focus_cursor = caret;
                            focus_cursor.offset = focus - anchor_cursor.offset;

                            let (before_selection, mid) = anchor_cursor.split_clone(&content);
                            let (in_selection, after_selection) = focus_cursor.split_clone(mid);

                            if (anchor_cursor.offset..focus_cursor.offset).contains(&caret.offset) {
                                caret.offset -= anchor_cursor.offset;
                                let (before_caret, after_caret) = caret.split_clone(in_selection);
                                let offsets = (
                                    0,
                                    before_selection.len(),
                                    before_selection.len() + before_caret.len() + after_caret.len()
                                );
                                view! {
                                    <PlainFragment offset=offsets.0>{before_selection}</PlainFragment>
                                    <Selected offset=offsets.1>{before_caret}<Caret/>{after_caret}</Selected>
                                    <PlainFragment offset=offsets.2>{after_selection}</PlainFragment>
                                }.into_any()
                            } else if caret.offset <= anchor_cursor.offset {
                                let (before_caret, after_caret) = caret.split_clone(before_selection);
                                let offsets = (
                                    0,
                                    before_caret.len(),
                                    before_caret.len() + after_caret.len(),
                                    before_caret.len() + after_caret.len() + in_selection.len()
                                );

                                view! {
                                    <PlainFragment offset=offsets.0>{before_caret}</PlainFragment>
                                    <Caret/>
                                    <PlainFragment offset=offsets.1>{after_caret}</PlainFragment>
                                    <Selected offset=offsets.2>
                                        {in_selection}
                                    </Selected>
                                    <PlainFragment offset=offsets.3>{after_selection}</PlainFragment>
                                }.into_any()
                            } else {
                                caret.offset -= focus_cursor.offset;
                                let (before_caret, after_caret) = caret.split_clone(after_selection);
                                let offsets = (
                                    0,
                                    before_selection.len(),
                                    before_selection.len() + in_selection.len(),
                                    before_selection.len() + in_selection.len() + after_caret.len()
                                );
                                view! {
                                    <PlainFragment offset=offsets.0>{before_selection}</PlainFragment>
                                    <Selected offset=offsets.1>{in_selection}</Selected>
                                    <PlainFragment offset=offsets.2>{before_caret}</PlainFragment>
                                    <Caret/>
                                    <PlainFragment offset=offsets.3>{after_caret}</PlainFragment>
                                }.into_any()
                            }
                        }
                    }
                }
                else {
                    match selection.is_plain_selected(content_id, &body.read_untracked()) {
                        Selection::Nothing => view! {{content}}.into_any(),
                        Selection::Span(anchor, focus) => {
                            let mut anchor_cursor = caret;
                            anchor_cursor.offset = anchor;

                            let mut focus_cursor = caret;
                            focus_cursor.offset = focus - anchor_cursor.offset;

                            let (before_selection, mid) = anchor_cursor.split_clone(&content);
                            let (in_selection, after_selection) = focus_cursor.split_clone(mid);

                            let offsets = (
                                0,
                                before_selection.len(),
                                before_selection.len() + in_selection.len()
                            );

                            view!{
                                <PlainFragment offset=offsets.0>{before_selection}</PlainFragment>
                                <Selected offset=offsets.1>{in_selection}</Selected>
                                <PlainFragment offset=offsets.2>{after_selection}</PlainFragment>
                            }.into_any()
                        }
                    }
                }
            }
            </span>
        }
    };

    view! ({render})
}
