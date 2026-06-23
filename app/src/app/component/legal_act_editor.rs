use leptos::prelude::*;
use leptos::reactive::{signal::RwSignal, traits::{Read, Update}};

use crate::app::component::{BlocMarianneInline, ButtonGroup, InlineEditableField};
use crate::model::{Annex, Article, ArticleKind, ConsideringId, LegalActBodyModel, LegalActModel, LegalActNodeData, LegalActeNodeId, VisaId};
use crate::utils::use_id;

#[component]
pub fn LegalActEditor<Act: LegalActModel + Send + Sync + 'static>(act: RwSignal<Act>) -> impl IntoView {
    let title = move || act.read().title().to_string();
    let considerings = move || act.read().iter_considerings().cloned().collect::<Vec<_>>();
    let visas = move || act.read().iter_visas().cloned().collect::<Vec<_>>();
    let body_root_id = move || act.read().borrow_body().root();

    let set_title = move |new_title: String| {
        act.update(move |act| act.set_title(new_title));
    }; 

    let add_visa = move |_| act.update(|act| act.add_visa("..."));
    let set_visa = move |id: &VisaId, contenu: String| act.update(|act| act.set_visa(id, contenu));
    let add_considering = move |_| act.update(|act| act.add_considering("..."));
    let set_considering = move |id: &ConsideringId, contenu: String| act.update(|act| act.set_considering(id, contenu));

    view ! {
        <div class="flex">
            <BlocMarianneInline 
                class="flex-1" 
                autorite={"Préfet\nde Seine-Maritime".to_string()}
            />
            <div class="font-bold flex-1 mt-[1em] text-[1.05em] uppercase">
                "Direction régionale de l’environnement, de l’aménagement et du logement"
            </div>
        </div>

        <h1 class="text-center my-24">
            <InlineEditableField 
                on_save={set_title} 
                value={title}
                class="text-4xl font-bold text-center"
            />
        </h1>

        <div class="print:p-0 py-2">
            <div class="flex print:hidden">
                <h2 class="flex-1 text-xl font-thin uppercase text-left">Visas</h2>
                <ButtonGroup>
                    <button on:click={add_visa}>+ visa</button>
                </ButtonGroup>
            </div>
            <ul class="list-none">
                <For
                    each=visas
                    key=|visa| visa.clone()
                    children=move |visa| view! {
                        <li class="*:first:font-bold *:first:mr-4"><span class="uppercase">VU</span> <span><InlineEditableField 
                            value={visa.contenu.clone()} 
                            on_save={move |nouveau_visa| set_visa(&visa.id, nouveau_visa)}
                        /></span>
                        </li>
                    }
                />
            </ul>
            
        </div>
        <div>
            <div class="flex print:hidden">
                <h2 class="flex-1 text-xl font-thin uppercase">Considérants</h2>
                <ButtonGroup>
                    <button on:click={add_considering}>+ considérant</button>
                </ButtonGroup>
            </div>
            <ul class="list-none">
                <For
                    each=considerings
                    key=|considering| considering.clone()
                    children=move |considering| view! {
                        <li class="*:first:font-bold *:first:mr-4"><span class="uppercase">Considérant</span> <InlineEditableField 
                            value={considering.contenu.clone()} 
                            on_save={move |nouveau_considérant| set_considering(&considering.id, nouveau_considérant)}
                        />
                        </li>
                    }
                />
            </ul>
        </div>

        <div class="font-bold text-2xl uppercase text-center my-4">Arrête</div>
        <NodeEditor act={act} node_id={body_root_id()}/>
    }
}

#[component]
fn NodeEditor<Act: LegalActModel + Send + Sync + 'static>(act: RwSignal<Act>, node_id: LegalActeNodeId) -> impl IntoView {
    use crate::model::LegalActNodeData::*;

    let acte_reader = act.read();
    let node =  node_id.get(acte_reader.borrow_body());

    match node.as_ref() {
        Body => view! {
            <BodyEditor act={act} node_id={node_id} />
        }.into_any(),
        Annex(_) => {
            view! {
                <AnnexEditor act={act} node_id={node_id} />
            }.into_any()
        },
        Chapter(_) => todo!(),
        Section(_) => todo!(),
        Article(_) => view! {
            <ArticleEditor act={act} node_id={node_id} />
        }.into_any(),
        List => todo!(),
        Paragraph => todo!(),
        Table => todo!(),
    }
}

#[component]
fn BodyEditor<Act: LegalActModel + Send + Sync + 'static>(act: RwSignal<Act>, node_id: LegalActeNodeId)  -> impl IntoView  {
    let children = move || node_id.children(act.read().borrow_body()).collect::<Vec<_>>();
    
    view! {
        <NodeTools act={act} node_id={node_id}/>
        <For each=children key=|child| *child children=move |child| view!{<NodeEditor act={act} node_id={child}/>} />
    }
}

#[component]
fn ArticleEditor<Act: LegalActModel + Send + Sync + 'static>(act: RwSignal<Act>, node_id: LegalActeNodeId) -> impl IntoView {
    let label = move || node_id.get(act.read().borrow_body()).as_ref_article().label.clone();
    let kind = move || node_id.get(act.read().borrow_body()).as_ref_article().kind.clone();
    let numerotation = move || node_id.get(act.read().borrow_body()).numerotation().iter().map(ToString::to_string).reduce(|acc, b| format!("{acc}.{b}"));

    let set_label = move |label| {
        act.update(|act| {
            let body = act.borrow_mut_body();
            node_id.get_mut(body).as_mut_article().label = label;
        });
    };

    view! {
        <h2 class="space-x-2">Article " " {numerotation} " " <InlineEditableField value={label} on_save={set_label}/></h2>
    }
}

#[component]
fn AnnexEditor<Act: LegalActModel + Send + Sync>(act: RwSignal<Act>, node_id: LegalActeNodeId) -> impl IntoView {
    use LegalActNodeData::*;

    
}

#[component]
fn NodeTools<Act: LegalActModel + Send + Sync + 'static>(act: RwSignal<Act>, node_id: LegalActeNodeId) -> impl IntoView {
    let ajouter_article = move |_| {
        let new_id = use_id();
        use ArticleKind::*;
        act.update(move |act| {
            node_id.append_child(
                Article::new(new_id(), "", PlainArticle), 
                act.borrow_mut_body()
            );
        });
    };

    let ajouter_annexe = move |_| {
        let new_id = use_id();
        act.update(move |act| {
            node_id.append_child(
                Annex::new(new_id(), ""), 
                act.borrow_mut_body()
            );
        });
    };

    view! {
        <ButtonGroup class="print:hidden">
            <button on:click={ajouter_article}>+ Article</button>
            <button on:click={ajouter_annexe}>+ Annexe</button>
            <button>+ Chapitre</button>
            <button>+ Section</button>
        </ButtonGroup>
    }
}
