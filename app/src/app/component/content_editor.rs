use std::{collections::VecDeque, ops::{Deref, DerefMut}};

use leptos::*;
use leptos::{ev::KeyboardEvent, html, prelude::*};
use strum_macros::AsRefStr;
use web_sys::{FocusEvent, InputEvent, MouseEvent};
use crate::{model::content::{Content, ContentBody, ContentId, ContentKind, Cursor}, polyfill::raycast_text_node, utils::{ID, IdGenerator}};

#[derive(Default, Debug, Clone, Copy)]
pub struct EditorSelection {
    state: EditorSelectionState,
    anchor: Option<Cursor>,
    focus: Option<Cursor>
}

impl EditorSelection {
    pub fn correct(&mut self, body: &ContentBody) {
        if let Some(focus) = self.focus && let Some(anchor) = self.anchor 
            && focus.partial_cmp(&anchor, body) == Some(std::cmp::Ordering::Less)
        {
            std::mem::swap(&mut self.anchor, &mut self.focus);
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, AsRefStr)]
pub enum EditorSelectionState {
    #[default]
    Idle,
    Dragging
}

impl EditorSelection {
    pub fn is_plain_selected(&self, content_id: ContentId, body: &ContentBody) -> Selection {
        let Some(anchor) = self.anchor else {return Selection::Nothing};
        let Some(focus) = self.focus else {return Selection::Nothing};

        if content_id.leaf_order(anchor.content_id, body) == Some(std::cmp::Ordering::Less) {
            return Selection::Nothing;
        }

        if content_id.leaf_order(focus.content_id, body) == Some(std::cmp::Ordering::Greater) {
            return Selection::Nothing;
        }

        if content_id == anchor.content_id && focus.content_id == content_id {
            return Selection::Span(anchor.offset, focus.offset)
        }

        if content_id == anchor.content_id {
            return Selection::Span(anchor.offset, content_id.len(body));
        }

        if content_id == focus.content_id {
            return Selection::Span(0, focus.offset);
        }

        return Selection::Span(0, content_id.len(body));
    }
}

pub enum Selection {
    Span(usize, usize),
    Nothing
}

#[derive(Clone, Copy)]
pub struct Editor {
    body: RwSignal<ContentBody>,
    cursor: RwSignal<EditorCursor>,
    selection: RwSignal<EditorSelection>,
    ev_queue: StoredValue<VecDeque<EditorEvent>>
}

pub enum EditorEvent {
    MouseDown(MouseEvent),
    MouseClick(MouseEvent),
    MouseUp(MouseEvent),
    MouseEnter(MouseEvent),
    MouseMove(MouseEvent),
    KeyDown(KeyboardEvent),
    AnchorSet(Cursor),
    FocusSet(Cursor),
    Focus,
    Blur,
    StringWritten(usize),
    CharAdded,
    CharRemoved
}

impl Editor {
    pub fn send_event(self, ev: EditorEvent) {
        self.ev_queue.update_value(|ev_queue| {
            ev_queue.push_back(ev);
            self.schedule_event_loop();
        });
    }

    pub fn schedule_event_loop(self) {
        request_animation_frame(move || {
            self.ev_queue.update_value(|ev_queue| {
                while let Some(event) = ev_queue.pop_front() {
                    self.process_event(event);
                }
            });
        });
    }
    pub fn process_event(self, ev: EditorEvent) {
        use EditorEvent::*;

        match ev {
            KeyDown(e) if e.key() == "ArrowLeft" => {
                self.move_cursor_to_left();
            },
            KeyDown(e) if e.key() == "ArrowRight" => {
                self.move_cursor_to_right();
            },
            KeyDown(e) if e.key() == "ArrowUp" => {},
            KeyDown(e) if e.key() == "ArrowDown" => {},
            KeyDown(e) if e.key() == "Backspace" => {
                self.body.update(|body| {
                    use Content::Plain;

                    let mut cursor = self.cursor.get();
                    cursor.left(body);

                    if cursor.offset == 0 {
                        if let Some(_) = cursor.content_id.prev_leaf(body) {
                            
                        }
                    }
                    
                    else if let Plain(text) = cursor.content_id.borrow_mut(body) {
                        let index = text.char_indices().nth(cursor.offset).unwrap().0;
                        text.remove(index);
                        self.send_event(EditorEvent::CharRemoved);
                    }
                    
                });
            },
            MouseDown(ev) => {
                self.selection.update(|sel| {
                    use EditorSelectionState::{Idle, Dragging};
                    if let Idle = sel.state {
                        let x = ev.x() as f32;
                        let y = ev.y() as f32;
                        
                        if let Some(cursor) = self.search_cursor_at(x, y) {
                            sel.state = Dragging;
                            sel.anchor = Some(cursor);
                            sel.focus = None;
                            self.send_event(EditorEvent::AnchorSet(cursor));
                            ev.prevent_default();
                        }
                    }
                });
            },
            MouseUp(_) => {
                self.selection.update(|sel| {
                    use EditorSelectionState::{Idle, Dragging};
                    if let Dragging = sel.state {
                        sel.state = Idle;
                    }
                })
            },
            MouseMove(ev) => {
                self.cursor.update(|cursor| {
                    let x = ev.x() as f32;
                    let y = ev.y() as f32;
                    
                    if let Some(mouse) = self.search_cursor_at(x, y) {
                        cursor.mouse = mouse;
                    }               
                });
                self.selection.update(|sel| {
                    use EditorSelectionState::Dragging;
                    if let Dragging = sel.state {
                        let x = ev.x() as f32;
                        let y = ev.y() as f32;
                        
                        if let Some(cursor) = self.search_cursor_at(x, y) {
                            sel.focus = Some(cursor);
                            sel.correct(&self.body.read_untracked());
                            self.send_event(EditorEvent::FocusSet(sel.focus.unwrap()));
                        }
                    }
                });
            }
            MouseClick(ev) => {
                let x = ev.x() as f32;
                let y = ev.y() as f32;

                if let Some(cursor) = self.search_cursor_at(x, y) {
                    self.move_cursor(cursor);
                }
            },
            Focus => self.cursor.update(|cursor| cursor.display = true),
            Blur => self.cursor.update(|cursor| cursor.display = false),
            StringWritten(len) => self.cursor.update(move |cursor| (0..len).into_iter().for_each(|_| cursor.right(&self.body.read_untracked()))),
            CharAdded => self.move_cursor_to_right(),
            CharRemoved => self.move_cursor_to_left(),
            FocusSet(cursor) => self.move_cursor(cursor),
            _ => {}
        }
    }

    pub fn write_str(self, str: &str) {
        self.body.update(|body| {
            use Content::Plain;

            let cursor = self.cursor.get();
            
            if let Plain(text) = cursor.content_id.borrow_mut(body) {
                let index = text.char_indices().nth(cursor.offset).unwrap().0;
                text.insert_str(index, str);
                self.send_event(EditorEvent::StringWritten(str.chars().count()));
            }
        }); 
    }

    pub fn search_cursor_at(self, x: f32, y: f32) -> Option<Cursor> {
        let document = document();
        let (node, offset) = raycast_text_node(&document, x, y)?;
        let mut node = node.parent_element()?;

        loop {
            if let Some(attr) = node.get_attribute("data-content-id") {
                let offset: usize = node.get_attribute("data-content-offset")
                    .and_then(|off| off.parse().ok())
                    .unwrap_or(offset) + offset;
                
                let content_id: ContentId = attr.parse().ok()?;
                return Some(Cursor { offset, content_id })
            } else {
                if let Some(parent) = node.parent_element() {
                    node = parent;
                } else {
                    break;
                }
            }
        }

        None
    }
    

    pub fn move_cursor(self, value: Cursor) {
        self.cursor.update(|cursor| {
            cursor.content_id = value.content_id;
            cursor.offset = value.offset;
        });
    }

    pub fn move_cursor_to_left(self) {
        let body = self.body.read_untracked();
        self.cursor.update(move |cursor| cursor.left(&body));
    }

    pub fn move_cursor_to_right(self) {
        let body = self.body.read_untracked();
        self.cursor.update(move |cursor| cursor.right(&body));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EditorCursor {
    id: ID,
    caret: Cursor,
    mouse: Cursor,
    display: bool
}

impl DerefMut for EditorCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.caret
    }
}

impl Deref for EditorCursor {
    type Target = Cursor;
    
    fn deref(&self) -> &Self::Target {
        &self.caret
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CurrentContentId(ContentId);

pub fn use_editor_content_body() -> RwSignal<ContentBody> {
    use_context::<Editor>().unwrap().body
}

pub fn use_editor_cursor() -> RwSignal<EditorCursor> {
    use_context::<Editor>().unwrap().cursor
}

pub fn use_editor_selection() -> RwSignal<EditorSelection> {
    use_context::<Editor>().unwrap().selection
}

pub fn use_editor() -> Editor {
    use_context::<Editor>().unwrap()
}

pub fn use_current_content_id() -> ContentId {
    use_context::<CurrentContentId>().unwrap().0
}

pub fn use_current_content() -> impl Fn() -> Content {
    let body = use_editor_content_body();
    let node_id = use_current_content_id();

    move || {
        node_id.borrow(&body.read()).clone()
    }
}

pub fn use_current_children() -> impl Fn() -> Vec<ContentId> {
    let body = use_editor_content_body();
    let node_id = use_current_content_id();

    move || {
        node_id.children(&body.read()).collect()
    }
}

#[component]
pub fn ContentEditor(value: ContentBody) -> impl IntoView {
    let cursor = EditorCursor {
        id: IdGenerator::new().next_id(),
        caret: Cursor { content_id: value.root.first_leaf(&value), offset: 0 },
        mouse: Cursor { content_id: value.root.first_leaf(&value), offset: 0 },
        display: false
    };

    let ev_queue = StoredValue::new(VecDeque::<EditorEvent>::new());

    let cursor = RwSignal::new(cursor);
    let body = RwSignal::new(value);
    let selection = RwSignal::new(EditorSelection::default());

    let editor = Editor {
        cursor, 
        body,  
        selection,
        ev_queue
    };

    provide_context(editor);
    provide_context(CurrentContentId(body.read_untracked().root));

    view! { <EditNode/> }
}

#[component]
pub fn EditNode() -> impl IntoView {
    use ContentKind::*;
    
    let body = use_editor_content_body();
    let content_id = use_current_content_id();

    let redirect = move || {
        match content_id.kind(&body.read()) {
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
    let content = use_current_content();
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
                            let (before_caret, after_caret) = content.split_clone_text_at(&caret);
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
   
                            let (before_selection, mid) = content.split_clone_text_at(&anchor_cursor);
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
                        Selection::Nothing => view! {{content.as_ref()}}.into_any(),
                        Selection::Span(anchor, focus) => {
                            let mut anchor_cursor = caret;
                            anchor_cursor.offset = anchor;

                            let mut focus_cursor = caret;
                            focus_cursor.offset = focus - anchor_cursor.offset;
   
                            let (before_selection, mid) = content.split_clone_text_at(&anchor_cursor);
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