pub mod component;

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};

use crate::{app::component::{ContentEditor, LegalActEditor}, model::{LegalActProject, content::ContentBody}, utils::provide_id_generator};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="fr">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();
    provide_id_generator();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/redacap.css"/>
        // sets the document title
        <Title text="Redac'AP"/>

        // content for this welcome page
        <Router>
            <main class="p-8 dark:bg-stone-900 dark:text-white">
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=PageEditeurProjetActeLegal/>
                    <Route path=StaticSegment("/content") view=PageContentEditor/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn PageContentEditor() -> impl IntoView {
    let mut content = ContentBody::new();
    
    content.root.append_content("Lorem ipsum dolor sit amet, consectetur adipiscing elit. Phasellus nunc metus, ultricies eget viverra nec, efficitur ut lectus. Integer viverra pulvinar pulvinar. Integer sit amet enim nec risus fermentum condimentum. Interdum et malesuada fames ac ante ipsum primis in faucibus. Morbi in ligula faucibus, ultrices sem vel, sagittis diam. Ut at ipsum ac mi tincidunt auctor. In efficitur velit id neque ultrices, in fringilla risus hendrerit. Interdum et malesuada fames ac ante ipsum primis in faucibus. Nam sed finibus mauris, vitae mattis lacus. Cras ut dui at dui convallis pellentesque. Pellentesque quis justo metus. Sed ac odio in urna faucibus molestie eget ac libero. Ut augue erat, commodo sed fringilla ut, tincidunt ac neque. Nam volutpat, ante nec placerat lobortis, orci ex mollis arcu, ac fermentum ligula enim ut tellus. Praesent semper suscipit mi nec dapibus. Aenean venenatis odio nec risus consequat, in finibus libero elementum.", &mut content);
    content.root.append_content("Integer fermentum lorem id nulla bibendum, sit amet fringilla lectus gravida. Curabitur pretium egestas massa, gravida tincidunt sapien sollicitudin vitae. Ut accumsan, turpis ut malesuada tincidunt, augue nisi cursus nisi, vitae tincidunt sapien erat vitae tortor. Donec ullamcorper, tellus non ullamcorper hendrerit, ipsum sem feugiat libero, vitae vulputate neque libero quis libero. Nulla sodales massa sed tellus facilisis sollicitudin. Ut a elit metus. Ut bibendum elementum turpis, facilisis cursus mauris porta et. Donec blandit enim leo, sed ultricies massa facilisis non. Morbi tincidunt egestas massa ut fringilla. Pellentesque habitant morbi tristique senectus et netus et malesuada fames ac turpis egestas. Quisque accumsan, turpis luctus vulputate porttitor, felis dolor scelerisque mi, id venenatis orci diam ac urna. Nam ac nisl nulla. In et arcu ligula. Curabitur facilisis metus vitae leo ornare tincidunt.", &mut content);
    view! {
        <ContentEditor value={content}/>
    }
}

#[component]
fn PageEditeurProjetActeLegal() -> impl IntoView {
    let act = RwSignal::new(LegalActProject::default());
    
    view! {
        <LegalActEditor act={act}/>
    }
}

