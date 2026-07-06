//! Implémentations des ports de `agent` (voir `agent::ports`) qui
//! branchent la boucle agentique sur la salle websocket courante : les
//! outils qui modifient l'acte agissent sur le [`YrsBody`] partagé de la
//! [`Room`] et diffusent la mise à jour Yrs résultante à tous les pairs
//! connectés ; les outils d'interaction relaient les questions de l'agent
//! au client à l'origine de la tâche et attendent sa réponse.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex as StdMutex};

use agent::ToolError;
use agent::ports::{
    LegalActEditorPort, Question, QuestionAnswer, UserInteractionPort, ValidationReport,
};
use agent::{AgentObserver, ToolCall};
use legal_act::{BodyNodeId, BodyRead, BodyWrite, NodeKind, NodeSpec, YrsBody};
use serde_json::Value;
use shared::id::ID;
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use yrs::{ReadTxn, Transact};

use super::protocol::{InteractionAnswerWire, InteractionQuestionWire, ServerMessage};
use super::state::EditorRoom;

/// Mot-clé accepté à la place d'un identifiant de nœud pour désigner la
/// racine de l'acte, afin que l'agent (ou l'utilisateur, via une tâche en
/// langage naturel) puisse y insérer du contenu sans jamais avoir à
/// connaître ni manipuler d'identifiant technique de nœud.
const ROOT_KEYWORD: &str = "root";

/// Mot-clé accepté à la place d'un identifiant de nœud pour désigner le
/// nœud actuellement ciblé par l'utilisateur dans l'éditeur (voir
/// [`WsLegalActEditor::selection`]).
const SELECTION_KEYWORD: &str = "selection";

/// Implémentation de [`LegalActEditorPort`] qui agit sur le [`YrsBody`]
/// d'une [`Room`] et diffuse chaque mutation aux pairs connectés.
pub struct WsLegalActEditor {
    room: Arc<EditorRoom>,
    /// Nœud actuellement ciblé par l'utilisateur dans l'éditeur de cette
    /// connexion (voir `ClientMessage::SetSelection` côté `crate::ws`),
    /// résolu par les outils de l'agent lorsqu'ils reçoivent le mot-clé
    /// [`SELECTION_KEYWORD`] plutôt qu'un identifiant explicite.
    selection: Arc<StdMutex<Option<BodyNodeId>>>,
    /// Utilisateur de la connexion à l'origine de la tâche agent en cours,
    /// au nom duquel chaque mutation est journalisée (voir
    /// [`EditorRoom::record_and_broadcast`]).
    author_id: ID,
}

impl WsLegalActEditor {
    #[must_use]
    pub fn new(
        room: Arc<EditorRoom>,
        selection: Arc<StdMutex<Option<BodyNodeId>>>,
        author_id: ID,
    ) -> Self {
        Self {
            room,
            selection,
            author_id,
        }
    }

    async fn state_vector(&self) -> yrs::StateVector {
        let body = self.room.body.lock().await;
        body.doc().transact().state_vector()
    }

    /// Diffuse aux pairs, et journalise au nom de [`Self::author_id`], la
    /// différence entre l'état `before` et l'état courant du document —
    /// c'est-à-dire la mise à jour Yrs produite par la mutation qui vient
    /// d'avoir lieu.
    async fn broadcast_diff(&self, before: &yrs::StateVector) {
        let diff = {
            let body = self.room.body.lock().await;
            body.doc().transact().encode_diff_v1(before)
        };
        self.room.record_and_broadcast(&self.author_id, diff).await;
    }

    /// Résout un identifiant de nœud fourni par l'agent : un identifiant
    /// technique explicite, ou l'un des mots-clés [`ROOT_KEYWORD`] /
    /// [`SELECTION_KEYWORD`] — ce qui évite d'exposer l'utilisateur aux
    /// identifiants internes des nœuds (l'agent n'a alors besoin ni de les
    /// connaître à l'avance, ni de les lui demander).
    async fn resolve(&self, raw: &str) -> Result<BodyNodeId, ToolError> {
        match raw {
            ROOT_KEYWORD => {
                let body = self.room.body.lock().await;
                Ok(body.root())
            }
            SELECTION_KEYWORD => {
                let current = *self.selection.lock().expect("verrou non empoisonné");
                current.ok_or_else(|| {
                    ToolError::InvalidArguments(
                        "aucun nœud n'est actuellement sélectionné dans l'éditeur : demande à \
                         l'inspecteur de cliquer sur « Cibler » sur le nœud voulu, ou utilise \
                         « root » pour viser la racine de l'acte"
                            .to_string(),
                    )
                })
            }
            _ => raw.parse().map_err(|error| {
                ToolError::InvalidArguments(format!("identifiant de nœud invalide : {error}"))
            }),
        }
    }
}

#[async_trait::async_trait]
impl LegalActEditorPort for WsLegalActEditor {
    async fn read_structure(&self) -> Result<Value, ToolError> {
        let body = self.room.body.lock().await;
        Ok(serialize_node(&*body, body.root()))
    }

    async fn fill_section(&self, section_id: &str, content: &str) -> Result<(), ToolError> {
        let id = self.resolve(section_id).await?;

        let before = self.state_vector().await;
        let outcome = {
            let mut body = self.room.body.lock().await;
            // `kind_of`/`first_leaf_of` paniquent si `id` est inconnu : on
            // capture ce cas plutôt que de faire planter la tâche agent.
            std::panic::catch_unwind(AssertUnwindSafe(|| fill_leaf(&mut body, id, content)))
        };
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(ToolError::Other(error.to_string())),
            Err(_) => return Err(ToolError::Other(format!("nœud introuvable : {section_id}"))),
        }
        self.broadcast_diff(&before).await;
        Ok(())
    }

    async fn insert_node(
        &self,
        parent_id: &str,
        kind: &str,
        content: Option<&str>,
    ) -> Result<String, ToolError> {
        let parent = self.resolve(parent_id).await?;
        let node_kind: NodeKind = kind.parse().map_err(|_| {
            ToolError::InvalidArguments(format!("type de noeud inconnu : « {kind} »"))
        })?;

        let before = self.state_vector().await;
        let outcome = {
            let mut body = self.room.body.lock().await;
            std::panic::catch_unwind(AssertUnwindSafe(|| {
                create_node(&mut body, parent, node_kind, content)
            }))
        };
        let id = match outcome {
            Ok(Ok(id)) => id,
            Ok(Err(error)) => return Err(ToolError::Other(error.to_string())),
            Err(_) => {
                return Err(ToolError::Other(format!(
                    "noeud parent introuvable : {parent_id}"
                )));
            }
        };
        self.broadcast_diff(&before).await;
        Ok(id.to_string())
    }

    async fn remove_node(&self, node_id: &str) -> Result<(), ToolError> {
        let id = self.resolve(node_id).await?;

        let before = self.state_vector().await;
        let outcome = {
            let mut body = self.room.body.lock().await;
            std::panic::catch_unwind(AssertUnwindSafe(|| body.remove_node(id)))
        };
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(ToolError::Other(error.to_string())),
            Err(_) => return Err(ToolError::Other(format!("noeud introuvable : {node_id}"))),
        }
        self.broadcast_diff(&before).await;
        Ok(())
    }

    async fn generate_numbering(&self) -> Result<(), ToolError> {
        let before = self.state_vector().await;
        let outcome = {
            let mut body = self.room.body.lock().await;
            let root = body.root();
            std::panic::catch_unwind(AssertUnwindSafe(|| renumber_tree(&mut *body, root)))
        };
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(ToolError::Other(error.to_string())),
            Err(_) => return Err(ToolError::Other("échec de la renumérotation".to_string())),
        }
        self.broadcast_diff(&before).await;
        Ok(())
    }

    async fn validate_structure(&self) -> Result<ValidationReport, ToolError> {
        let body = self.room.body.lock().await;
        let mut errors = Vec::new();
        check_structure(&*body, body.root(), &mut errors);
        Ok(ValidationReport { errors })
    }

    async fn read_title(&self) -> Result<String, ToolError> {
        let body = self.room.body.lock().await;
        Ok(body.title())
    }

    async fn set_title(&self, title: &str) -> Result<(), ToolError> {
        let before = self.state_vector().await;
        {
            let mut body = self.room.body.lock().await;
            body.set_title(title);
        }
        self.broadcast_diff(&before).await;
        Ok(())
    }
}

/// Sérialise récursivement `id` et son sous-arbre en `{ id, kind, number?,
/// text?, children? }`, pour l'outil `read_structure` : `number` n'apparaît
/// que pour les nœuds numérotés, `text` que pour les nœuds `Plain`, et
/// `children` que pour les nœuds ayant au moins un enfant.
fn serialize_node(body: &YrsBody, id: BodyNodeId) -> Value {
    let kind = body.kind_of(id);
    let mut node = serde_json::json!({ "id": id.to_string(), "kind": kind.to_string() });

    if let Some(number) = body.spec_of(id).number() {
        node["number"] = serde_json::json!(number);
    }
    if kind == NodeKind::Plain {
        node["text"] = serde_json::json!(body.text_of(id));
    }

    let children = body.children_of(id);
    if !children.is_empty() {
        node["children"] = serde_json::json!(
            children
                .into_iter()
                .map(|child| serialize_node(body, child))
                .collect::<Vec<_>>()
        );
    }
    node
}

/// Nœud à remplir pour `id` : son conteneur de contenu (ex. `ArticleBody`
/// pour un `Article`, voir [`legal_act::NodeKind::content_container_kind`])
/// s'il en a un, sinon `id` lui-même. Évite qu'un contenu généré pour un
/// `Article` atterrisse dans son `LibelleArticle` plutôt que dans son corps.
fn content_target(body: &YrsBody, id: BodyNodeId) -> BodyNodeId {
    body.kind_of(id)
        .content_container_kind()
        .and_then(|container_kind| {
            body.children_of(id)
                .into_iter()
                .find(|&c| body.kind_of(c) == container_kind)
        })
        .unwrap_or(id)
}

/// Remplit la feuille `Plain` de `id` (ou de son conteneur de contenu, voir
/// [`content_target`] ; ou `id` lui-même s'il s'agit déjà d'un nœud `Plain`)
/// avec `content`.
fn fill_leaf(body: &mut YrsBody, id: BodyNodeId, content: &str) -> anyhow::Result<()> {
    let target = content_target(body, id);
    let leaf = if body.kind_of(target) == NodeKind::Plain {
        target
    } else {
        body.first_leaf_of(target)
    };
    if body.kind_of(leaf) != NodeKind::Plain {
        anyhow::bail!("le nœud {id} n'a pas de contenu textuel modifiable");
    }
    body.set_spec_unchecked(leaf, NodeSpec::Plain(content.to_string()))
}

/// Crée un nœud de type `kind` sous `parent` (voir
/// [`legal_act::BodyWrite::append_node`] pour les invariants respectés :
/// enfants autorisés, ordre sous Root, feuilles `Plain` obligatoires), et y
/// insère `content` le cas échéant via [`fill_leaf`].
fn create_node(
    body: &mut YrsBody,
    parent: BodyNodeId,
    kind: NodeKind,
    content: Option<&str>,
) -> anyhow::Result<BodyNodeId> {
    let id = body.append_node(parent, kind.default_spec())?;
    if let Some(content) = content {
        fill_leaf(body, id, content)?;
    }
    Ok(id)
}

/// Recalcule la numérotation de tous les nœuds numérotés de l'arbre, en
/// réutilisant l'invariant [`legal_act::BodyWrite::renumber_siblings`] pour
/// chaque groupe de frères de même type numéroté.
fn renumber_tree(body: &mut YrsBody, node: BodyNodeId) -> anyhow::Result<()> {
    let mut seen: Vec<NodeKind> = Vec::new();
    for child in body.children_of(node) {
        let kind = body.kind_of(child);
        if kind.is_numbered() && !seen.contains(&kind) {
            seen.push(kind);
            body.renumber_siblings(node, kind)?;
        }
    }
    for child in body.children_of(node) {
        renumber_tree(body, child)?;
    }
    Ok(())
}

/// Vérifie récursivement les mêmes invariants structurels que
/// [`legal_act::BodyWrite::append_node`] impose à la construction :
/// types d'enfants autorisés et ordre des groupes sous `Root`.
fn check_structure(body: &YrsBody, node: BodyNodeId, errors: &mut Vec<String>) {
    let kind = body.kind_of(node);
    let children = body.children_of(node);
    let mut last_group = 0u8;
    for child in children {
        let child_kind = body.kind_of(child);
        if !kind.can_accept_child(child_kind) {
            errors.push(format!(
                "{child} ({child_kind}) n'est pas un enfant valide de {node} ({kind})"
            ));
        }
        if kind == NodeKind::Root {
            let group = child_kind.root_order_group().unwrap_or(u8::MAX);
            if group < last_group {
                errors.push(format!(
                    "ordre invalide dans Root : {child_kind} après un groupe supérieur"
                ));
            }
            last_group = group;
        }
        check_structure(body, child, errors);
    }
}

/// Implémentation de [`UserInteractionPort`] qui relaie les questions de
/// l'agent au client websocket à l'origine de la tâche en cours, et
/// attend sa réponse sur le canal de contrôle du protocole.
pub struct WsUserInteraction {
    out: mpsc::UnboundedSender<ServerMessage>,
    answers: AsyncMutex<mpsc::UnboundedReceiver<serde_json::Value>>,
}

impl WsUserInteraction {
    #[must_use]
    pub fn new(
        out: mpsc::UnboundedSender<ServerMessage>,
        answers: mpsc::UnboundedReceiver<serde_json::Value>,
    ) -> Self {
        Self {
            out,
            answers: AsyncMutex::new(answers),
        }
    }

    async fn recv_answer(&self) -> Result<serde_json::Value, ToolError> {
        self.answers
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| ToolError::Other("connexion websocket fermée avant réponse".to_string()))
    }
}

#[async_trait::async_trait]
impl UserInteractionPort for WsUserInteraction {
    async fn ask(&self, question: &str) -> Result<String, ToolError> {
        self.out
            .send(ServerMessage::InteractionAsk {
                question: question.to_string(),
            })
            .map_err(|_| ToolError::Other("connexion websocket fermée".to_string()))?;
        let value = self.recv_answer().await?;
        serde_json::from_value(value)
            .map_err(|error| ToolError::Other(format!("réponse invalide à la question : {error}")))
    }

    async fn confirm(&self, message: &str) -> Result<bool, ToolError> {
        self.out
            .send(ServerMessage::InteractionConfirm {
                message: message.to_string(),
            })
            .map_err(|_| ToolError::Other("connexion websocket fermée".to_string()))?;
        let value = self.recv_answer().await?;
        serde_json::from_value(value).map_err(|error| {
            ToolError::Other(format!("réponse invalide à la confirmation : {error}"))
        })
    }

    async fn ask_questions(
        &self,
        prompt: &str,
        questions: &[Question],
    ) -> Result<Vec<QuestionAnswer>, ToolError> {
        let wire_questions = questions
            .iter()
            .map(|q| InteractionQuestionWire {
                id: q.id.clone(),
                label: q.label.clone(),
                options: q.options.clone(),
            })
            .collect();
        self.out
            .send(ServerMessage::InteractionQuestions {
                prompt: prompt.to_string(),
                questions: wire_questions,
            })
            .map_err(|_| ToolError::Other("connexion websocket fermée".to_string()))?;
        let value = self.recv_answer().await?;
        let answers: Vec<InteractionAnswerWire> =
            serde_json::from_value(value).map_err(|error| {
                ToolError::Other(format!("réponses invalides au formulaire : {error}"))
            })?;
        Ok(answers
            .into_iter()
            .map(|a| QuestionAnswer {
                question_id: a.question_id,
                value: a.value,
                unsatisfactory_reason: a.unsatisfactory_reason,
            })
            .collect())
    }
}

/// Relaie au client websocket à l'origine de la tâche les réflexions du
/// modèle et les appels d'outils que l'agent déclenche (voir
/// `agent::AgentObserver`), pour affichage dans `agent::AgentPanel` au fil
/// de l'eau plutôt qu'une fois la tâche entièrement terminée.
#[async_trait::async_trait]
impl AgentObserver for WsUserInteraction {
    async fn on_reasoning_delta(&self, delta: &str) {
        let _ = self.out.send(ServerMessage::AgentReasoningDelta {
            delta: delta.to_string(),
        });
    }

    async fn on_content_delta(&self, delta: &str) {
        let _ = self.out.send(ServerMessage::AgentContentDelta {
            delta: delta.to_string(),
        });
    }

    async fn on_turn_finished(&self) {
        let _ = self.out.send(ServerMessage::AgentStepFinished);
    }

    async fn on_tool_call_started(&self, call: &ToolCall) {
        let _ = self.out.send(ServerMessage::AgentToolCallStarted {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        });
    }

    async fn on_tool_call_finished(&self, call_id: &str, result: &Result<String, String>) {
        let (ok, output) = match result {
            Ok(output) => (true, output.clone()),
            Err(message) => (false, message.clone()),
        };
        let _ = self.out.send(ServerMessage::AgentToolCallFinished {
            id: call_id.to_string(),
            ok,
            output,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pool paresseux (aucune connexion réseau) : la salle de test n'a pas
    /// d'acte légal associé (`legal_act_id: None`), donc `record_and_broadcast`
    /// ne touche jamais ce pool — seule sa présence en tant que valeur est
    /// nécessaire pour construire une `EditorRoom`.
    fn test_pool() -> storage::Pool {
        storage::connect_lazy("postgres://localhost/unused").expect("pool paresseux valide")
    }

    fn room_with_article() -> (Arc<EditorRoom>, BodyNodeId) {
        let mut body = YrsBody::new();
        let root = body.root();
        let article = body
            .append_node(root, NodeSpec::Article(legal_act::Article::default()))
            .unwrap();
        (EditorRoom::new(test_pool(), None, body, 1), article)
    }

    /// Construit un éditeur sans nœud sélectionné, pour les tests qui
    /// n'exercent pas la résolution du mot-clé `"selection"`.
    fn new_editor(room: &Arc<EditorRoom>) -> WsLegalActEditor {
        WsLegalActEditor::new(
            room.clone(),
            Arc::new(StdMutex::new(None)),
            shared::id::generate_id(),
        )
    }

    /// Feuille `Plain` du corps (`ArticleBody`) d'un article, où le contenu
    /// généré par `fill_section`/`insert_node` doit atterrir en priorité,
    /// par opposition à son `LibelleArticle`.
    fn article_body_leaf(body: &YrsBody, article: BodyNodeId) -> BodyNodeId {
        let article_body = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
            .expect("ArticleBody manquant");
        body.first_leaf_of(article_body)
    }

    #[tokio::test]
    async fn read_structure_exposes_ids_kinds_numbers_and_text() {
        let (room, article) = room_with_article();
        {
            let mut body = room.body.lock().await;
            let leaf = body.first_leaf_of(article);
            body.set_spec_unchecked(leaf, NodeSpec::Plain("Contenu de l'article".to_string()))
                .unwrap();
        }
        let editor = new_editor(&room);

        let structure = editor.read_structure().await.unwrap();

        let root_id = { room.body.lock().await.root().to_string() };
        assert_eq!(structure["id"], root_id);
        assert_eq!(structure["kind"], "Root");

        let article_node = &structure["children"][0];
        assert_eq!(article_node["id"], article.to_string());
        assert_eq!(article_node["kind"], "Article");
        assert_eq!(article_node["number"], 1);

        let leaf_text = article_node["children"][0]["children"][0]["text"]
            .as_str()
            .unwrap();
        assert_eq!(leaf_text, "Contenu de l'article");
    }

    #[tokio::test]
    async fn fill_section_updates_leaf_and_broadcasts_diff() {
        let (room, article) = room_with_article();
        let mut updates = room.updates.subscribe();
        let editor = new_editor(&room);

        editor
            .fill_section(&article.to_string(), "Contenu de l'article")
            .await
            .unwrap();

        let diff = updates
            .recv()
            .await
            .expect("une mise à jour doit être diffusée");
        assert!(!diff.is_empty());

        let body = room.body.lock().await;
        let leaf = article_body_leaf(&body, article);
        assert_eq!(body.text_of(leaf), "Contenu de l'article");
    }

    #[tokio::test]
    async fn fill_section_rejects_unknown_node() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);

        let result = editor
            .fill_section(&BodyNodeId::new().to_string(), "x")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn insert_node_creates_child_and_broadcasts_diff() {
        let (room, _) = room_with_article();
        let root = { room.body.lock().await.root() };
        let mut updates = room.updates.subscribe();
        let editor = new_editor(&room);

        let new_id = editor
            .insert_node(&root.to_string(), "Article", Some("Contenu de l'article"))
            .await
            .unwrap();

        let diff = updates
            .recv()
            .await
            .expect("une mise à jour doit être diffusée");
        assert!(!diff.is_empty());

        let body = room.body.lock().await;
        let id: BodyNodeId = new_id.parse().unwrap();
        assert_eq!(body.kind_of(id), NodeKind::Article);
        let leaf = article_body_leaf(&body, id);
        assert_eq!(body.text_of(leaf), "Contenu de l'article");
    }

    #[tokio::test]
    async fn insert_node_rejects_unknown_kind() {
        let (room, _) = room_with_article();
        let root = { room.body.lock().await.root() };
        let editor = new_editor(&room);

        let result = editor
            .insert_node(&root.to_string(), "PasUnType", None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn insert_node_rejects_disallowed_child() {
        let (room, article) = room_with_article();
        let editor = new_editor(&room);

        // Un Article n'est pas un enfant autorisé d'un Article.
        let result = editor
            .insert_node(&article.to_string(), "Article", None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn insert_node_accepts_root_keyword_without_exposing_node_ids() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);

        let new_id = editor.insert_node("root", "Article", None).await.unwrap();

        let body = room.body.lock().await;
        let root = body.root();
        let id: BodyNodeId = new_id.parse().unwrap();
        assert!(body.children_of(root).contains(&id));
    }

    #[tokio::test]
    async fn fill_section_accepts_selection_keyword() {
        let (room, article) = room_with_article();
        let selection = Arc::new(StdMutex::new(Some(article)));
        let editor = WsLegalActEditor::new(room.clone(), selection, shared::id::generate_id());

        editor
            .fill_section("selection", "Contenu ciblé")
            .await
            .unwrap();

        let body = room.body.lock().await;
        let leaf = article_body_leaf(&body, article);
        assert_eq!(body.text_of(leaf), "Contenu ciblé");
    }

    #[tokio::test]
    async fn selection_keyword_rejected_when_nothing_is_targeted() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);

        let result = editor.fill_section("selection", "x").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_node_deletes_subtree_and_broadcasts_diff() {
        let (room, article) = room_with_article();
        let mut updates = room.updates.subscribe();
        let editor = new_editor(&room);

        editor.remove_node(&article.to_string()).await.unwrap();

        let diff = updates
            .recv()
            .await
            .expect("une mise à jour doit être diffusée");
        assert!(!diff.is_empty());

        let body = room.body.lock().await;
        let root = body.root();
        assert!(body.children_of(root).is_empty());
    }

    #[tokio::test]
    async fn remove_node_rejects_unknown_node() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);

        let result = editor.remove_node(&BodyNodeId::new().to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_numbering_renumbers_sibling_articles() {
        let (room, _) = room_with_article();
        {
            let mut body = room.body.lock().await;
            let root = body.root();
            body.append_node(root, NodeSpec::Article(legal_act::Article::default()))
                .unwrap();
        }
        let editor = new_editor(&room);

        editor.generate_numbering().await.unwrap();

        let body = room.body.lock().await;
        let root = body.root();
        let numbers: Vec<u32> = body
            .children_of(root)
            .into_iter()
            .filter(|&id| body.kind_of(id) == NodeKind::Article)
            .map(|id| body.spec_of(id).number().unwrap())
            .collect();
        assert_eq!(numbers, vec![1, 2]);
    }

    #[tokio::test]
    async fn read_title_defaults_to_empty() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);

        assert_eq!(editor.read_title().await.unwrap(), "");
    }

    #[tokio::test]
    async fn set_title_updates_title_and_broadcasts_diff() {
        let (room, _) = room_with_article();
        let mut updates = room.updates.subscribe();
        let editor = new_editor(&room);

        editor
            .set_title("Arrêté préfectoral portant autorisation d'exploiter")
            .await
            .unwrap();

        let diff = updates
            .recv()
            .await
            .expect("une mise à jour doit être diffusée");
        assert!(!diff.is_empty());

        assert_eq!(
            editor.read_title().await.unwrap(),
            "Arrêté préfectoral portant autorisation d'exploiter"
        );
    }

    #[tokio::test]
    async fn validate_structure_reports_no_errors_on_valid_tree() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);

        let report = editor.validate_structure().await.unwrap();
        assert!(report.is_valid());
    }
}
