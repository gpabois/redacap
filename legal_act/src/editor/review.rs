/// Composants Leptos pour l'affichage et la saisie des commentaires
/// (review) d'un projet d'acte légal.
///
/// Les commentaires sont ancrés à une [`crate::cursor::Selection`] ou non et
/// peuvent être résolus ou recevoir des réponses arborescentes.
use leptos::prelude::*;

use dsfr::{Button, ButtonVariant, Size, Textarea};

use super::context::expect_editor_context;
use super::state::PendingComment;
use crate::traits::review::{Comment, ReviewRead, ReviewWrite};

/// Panneau listant tous les commentaires du projet (racines, avec leurs
/// réponses imbriquées), et le compositeur pour en créer un nouveau.
#[component]
pub fn ReviewPanel() -> impl IntoView {
    let ctx = expect_editor_context();

    let root_comments = Signal::derive(move || ctx.reviews.with(|r| r.root_comments()));
    let unresolved_count =
        Signal::derive(move || root_comments.get().iter().filter(|c| !c.resolved).count());

    view! {
        <div class="review-panel flex flex-col gap-2 p-2 overflow-y-auto">
            <PendingCommentComposer/>
            <div class="text-xs text-gray-500 dark:text-gray-400">
                {move || match unresolved_count.get() {
                    0 => "Aucun commentaire non résolu".to_string(),
                    1 => "1 commentaire non résolu".to_string(),
                    n => format!("{n} commentaires non résolus"),
                }}
            </div>
            <For
                each=move || root_comments.get()
                key=|c| c.id
                children=|c| view! { <CommentThread comment=c/> }
            />
        </div>
    }
}

/// Compositeur de nouveau commentaire : bouton "+ Commentaire" tant que
/// [`EditorContext::pending_comment`](super::context::EditorContext::pending_comment)
/// est vide, formulaire (avec rappel de l'extrait sélectionné le cas
/// échéant) une fois ouvert.
#[component]
fn PendingCommentComposer() -> impl IntoView {
    let ctx = expect_editor_context();
    let text = RwSignal::new(String::new());

    view! {
        <div class="border border-gray-200 dark:border-gray-700 rounded p-2">
            <Show
                when=move || ctx.pending_comment.get().is_some()
                fallback=move || view! {
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=Size::Sm
                        disabled=ctx.current_user.get_untracked().is_none()
                        on_click=move |_| {
                            ctx.pending_comment.set(Some(PendingComment::default()));
                        }
                    >
                        "+ Commentaire"
                    </Button>
                }
            >
                {move || ctx.pending_comment.get().map(|pending| {
                    let excerpt = pending.excerpt.clone();
                    view! {
                        <div class="flex flex-col gap-2">
                            {excerpt.clone().map(|e| view! {
                                <blockquote class="border-l-2 border-blue-france pl-2 italic text-xs text-gray-500 dark:text-gray-400">
                                    {format!("« {e} »")}
                                </blockquote>
                            })}
                            <Textarea
                                label="Nouveau commentaire"
                                value=Signal::derive(move || text.get())
                                on_input=move |v| text.set(v)
                                rows=3
                            />
                            <div class="flex gap-2">
                                <Button
                                    size=Size::Sm
                                    on_click=move |_| {
                                        let value = text.get_untracked();
                                        if value.trim().is_empty() {
                                            return;
                                        }
                                        let Some(author) = ctx.current_user.get_untracked() else {
                                            return;
                                        };
                                        let mut comment = Comment::new(author, value.trim());
                                        if let (Some(selection), Some(excerpt)) =
                                            (pending.selection, pending.excerpt.clone())
                                        {
                                            comment = comment.with_selection(selection, excerpt);
                                        }
                                        ctx.reviews.update(|r| {
                                            r.add_comment(comment);
                                        });
                                        text.set(String::new());
                                        ctx.pending_comment.set(None);
                                    }
                                >
                                    "Publier"
                                </Button>
                                <Button
                                    variant=ButtonVariant::TertiaryNoOutline
                                    size=Size::Sm
                                    on_click=move |_| {
                                        ctx.pending_comment.set(None);
                                        text.set(String::new());
                                    }
                                >
                                    "Annuler"
                                </Button>
                            </div>
                        </div>
                    }
                })}
            </Show>
        </div>
    }
}

/// Affiche un commentaire, l'extrait auquel il est ancré le cas échéant,
/// ses actions (répondre / résoudre / supprimer selon les droits de
/// l'utilisateur courant) et ses réponses imbriquées.
#[component]
pub fn CommentThread(comment: Comment) -> impl IntoView {
    let ctx = expect_editor_context();
    let comment_id = comment.id;
    let author = comment.author.clone();
    let text = comment.text.clone();
    let excerpt = comment.excerpt.clone();
    let resolved = comment.resolved;

    let author_for_delete = author.clone();
    let author_for_resolve = author.clone();

    let can_delete = move || {
        ctx.current_user
            .get()
            .is_some_and(|user| user == author_for_delete)
    };
    let can_resolve = move || {
        !resolved
            && ctx
                .current_user
                .get()
                .is_some_and(|user| user == author_for_resolve || ctx.can_edit.get())
    };

    let replies = Signal::derive(move || ctx.reviews.with(|r| r.replies_to(comment_id)));

    let reply_open = RwSignal::new(false);
    let reply_text = RwSignal::new(String::new());

    view! {
        <div
            class="comment-thread border border-gray-200 dark:border-gray-700 rounded p-2 mb-2"
            class:opacity-60=resolved
        >
            <div class="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400 mb-1">
                <span class="font-semibold text-gray-700 dark:text-gray-200">{author.clone()}</span>
                {resolved.then(|| view! {
                    <span class="text-teal-600 dark:text-teal-400 font-medium">"Résolu"</span>
                })}
            </div>
            {excerpt.map(|e| view! {
                <blockquote class="border-l-2 border-blue-france pl-2 italic text-xs text-gray-500 dark:text-gray-400 mb-1">
                    {format!("« {e} »")}
                </blockquote>
            })}
            <p class="text-sm mb-1 whitespace-pre-wrap">{text}</p>
            <div class="flex gap-3 text-xs">
                <button
                    type="button"
                    class="text-blue-france dark:text-blue-france-925 hover:underline cursor-pointer"
                    on:click=move |_| reply_open.update(|v| *v = !*v)
                >
                    "Répondre"
                </button>
                <Show when=can_resolve>
                    <button
                        type="button"
                        class="text-teal-600 dark:text-teal-400 hover:underline cursor-pointer"
                        on:click=move |_| {
                            let Some(actor) = ctx.current_user.get_untracked() else { return };
                            let can_edit = ctx.can_edit.get_untracked();
                            ctx.reviews.update(|r| {
                                let _ = r.try_resolve_comment(comment_id, &actor, can_edit);
                            });
                        }
                    >
                        "Marquer résolu"
                    </button>
                </Show>
                <Show when=can_delete>
                    <button
                        type="button"
                        class="text-red-500 hover:underline cursor-pointer"
                        on:click=move |_| {
                            let Some(actor) = ctx.current_user.get_untracked() else { return };
                            ctx.reviews.update(|r| {
                                let _ = r.try_delete_comment(comment_id, &actor);
                            });
                        }
                    >
                        "Supprimer"
                    </button>
                </Show>
            </div>

            <Show when=move || reply_open.get()>
                <div class="mt-2 flex flex-col gap-1">
                    <Textarea
                        label="Réponse"
                        value=Signal::derive(move || reply_text.get())
                        on_input=move |v| reply_text.set(v)
                        rows=2
                    />
                    <div class="flex gap-2">
                        <Button
                            size=Size::Sm
                            on_click=move |_| {
                                let value = reply_text.get_untracked();
                                if value.trim().is_empty() {
                                    return;
                                }
                                let Some(author) = ctx.current_user.get_untracked() else {
                                    return;
                                };
                                ctx.reviews.update(|r| {
                                    r.add_comment(Comment::new(author, value.trim()).reply_to(comment_id));
                                });
                                reply_text.set(String::new());
                                reply_open.set(false);
                            }
                        >
                            "Répondre"
                        </Button>
                        <Button
                            variant=ButtonVariant::TertiaryNoOutline
                            size=Size::Sm
                            on_click=move |_| {
                                reply_open.set(false);
                                reply_text.set(String::new());
                            }
                        >
                            "Annuler"
                        </Button>
                    </div>
                </div>
            </Show>

            <div class="ml-3 mt-2 border-l border-gray-100 dark:border-gray-800 pl-2">
                <For
                    each=move || replies.get()
                    key=|c: &Comment| c.id
                    // `.into_any()` : casse le cycle de type opaque récursif
                    // (`CommentThread` s'affiche lui-même pour ses réponses).
                    children=|c| view! { <CommentThread comment=c/> }.into_any()
                />
            </div>
        </div>
    }
}
