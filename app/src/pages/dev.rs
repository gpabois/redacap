use legal_act::{LegalActEditor, LegalActProject};
use leptos::prelude::*;


#[component]
pub fn PageDevLegalActEditor() -> impl IntoView {
    let project = LegalActProject::default();

    let _ = project.title().update("Arrêté du xx/xx/xxxx portant ...", Default::default());
    project.append_visa("le code de l'environnement, et notamment ses articles L. 511-1 et L. 171-7");
    project.append_considerant("les non-conformités relevées lors du contrôle");

    view! {
        <LegalActEditor act={project}/>
    }
}