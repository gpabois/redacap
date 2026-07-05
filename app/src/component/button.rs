use leptos::prelude::*;
use leptos::{IntoView, component, view};

#[component]
pub fn ButtonGroup(#[prop(optional)] class: &'static str, children: Children) -> impl IntoView {
    view! {
        <span class=format!("{class}
            inline-flex 
            *:text-xs 
            divide-x 
            *:cursor-pointer 
            *:border-1 
            *:border-teal-600 
            *:first:rounded-l-sm 
            *:last:rounded-r-sm 
            *:p-1
            ")>
            {children()}
        </span>
    }
}
