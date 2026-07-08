//! Implémentations des ports de `agent` (voir `agent::ports`) qui
//! branchent l'orchestration sur la salle websocket courante : les outils
//! qui modifient l'acte agissent sur le [`YrsBody`] partagé de la [`Room`]
//! et diffusent la mise à jour Yrs résultante à tous les pairs connectés.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex as StdMutex};

use agent::ToolError;
use agent::ports::{
    DocumentContent, DocumentContentPort, IntentionPort, IntentionSummary, LegalActEditorPort,
    ValidationReport,
};
use agent::{AgentObserver, ToolCall};
use content::{List, ListItem, ListMarker, Span};
use legal_act::{BodyNodeId, BodyRead, BodyWrite, NodeKind, NodeSpec, YrsBody};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;
use shared::id::ID;
use tokio::sync::broadcast;
use yrs::{ReadTxn, Transact};

use super::protocol::ServerMessage;
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

/// Implémentation de [`IntentionPort`] qui associe/retire des intentions du
/// domaine au projet `legal_act_id`, en journalisant chaque changement de la
/// même façon que le panneau « Paramètres » de l'éditeur (voir
/// `app::pages::project_intentions`) : seules les intentions du domaine du
/// projet peuvent lui être associées.
pub struct WsIntentions {
    pool: storage::Pool,
    legal_act_id: ID,
    author_id: ID,
}

impl WsIntentions {
    #[must_use]
    pub fn new(pool: storage::Pool, legal_act_id: ID, author_id: ID) -> Self {
        Self {
            pool,
            legal_act_id,
            author_id,
        }
    }

    async fn audit(&self, action: &str, intention_id: ID) {
        let _ = storage::audit_log::record_audit_event(
            &self.pool,
            shared::model::CreateAuditEvent {
                actor_id: Some(self.author_id),
                actor_ip: None,
                action: action.to_string(),
                resource_type: "legal_act_intention".to_string(),
                resource_id: Some(intention_id),
                details: None,
            },
        )
        .await;
    }
}

#[async_trait::async_trait]
impl IntentionPort for WsIntentions {
    async fn list(&self) -> Result<Vec<IntentionSummary>, ToolError> {
        let legal_act = storage::legal_act::get_legal_act(&self.pool, &self.legal_act_id)
            .await
            .map_err(|error| ToolError::Other(error.to_string()))?;
        let domain_intentions =
            storage::intention::list_intentions_by_domain(&self.pool, &legal_act.domain_id)
                .await
                .map_err(|error| ToolError::Other(error.to_string()))?;
        let attached =
            storage::intention::list_intentions_for_legal_act(&self.pool, &self.legal_act_id)
                .await
                .map_err(|error| ToolError::Other(error.to_string()))?;
        let attached_ids: std::collections::HashSet<ID> =
            attached.into_iter().map(|intention| intention.id).collect();

        Ok(domain_intentions
            .into_iter()
            .map(|intention| IntentionSummary {
                attached: attached_ids.contains(&intention.id),
                id: intention.id.to_string(),
                name: intention.name,
            })
            .collect())
    }

    async fn add(&self, intention_id: &str) -> Result<(), ToolError> {
        let intention_id: ID = intention_id.parse().map_err(|_| {
            ToolError::InvalidArguments("identifiant d'intention invalide".to_string())
        })?;

        let legal_act = storage::legal_act::get_legal_act(&self.pool, &self.legal_act_id)
            .await
            .map_err(|error| ToolError::Other(error.to_string()))?;
        let intention = storage::intention::get_intention(&self.pool, &intention_id)
            .await
            .map_err(|_| ToolError::InvalidArguments("intention introuvable".to_string()))?;
        if intention.domain_id != legal_act.domain_id {
            return Err(ToolError::InvalidArguments(
                "cette intention n'appartient pas au domaine du projet".to_string(),
            ));
        }

        storage::intention::add_intention_to_legal_act(
            &self.pool,
            &self.legal_act_id,
            &intention_id,
        )
        .await
        .map_err(|error| ToolError::Other(error.to_string()))?;
        self.audit("add", intention_id).await;
        Ok(())
    }

    async fn remove(&self, intention_id: &str) -> Result<(), ToolError> {
        let intention_id: ID = intention_id.parse().map_err(|_| {
            ToolError::InvalidArguments("identifiant d'intention invalide".to_string())
        })?;

        storage::intention::remove_intention_from_legal_act(
            &self.pool,
            &self.legal_act_id,
            &intention_id,
        )
        .await
        .map_err(|error| ToolError::Other(error.to_string()))?;
        self.audit("remove", intention_id).await;
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

/// Remplace le contenu du nœud résolu depuis `id` (voir [`content_target`])
/// par le résultat du parsing Markdown de `content` : **gras**/*italique*/
/// ~~barré~~ deviennent des `Span`, les lignes vides séparent des
/// `Paragraphe`, les listes à puces/numérotées et les tableaux GFM
/// deviennent des `List`/`Table` — dans la limite de ce que le nœud visé
/// accepte structurellement (voir [`NodeKind::CHILDREN_TABLE`]) :
/// - un nœud acceptant des blocs (ex. `ArticleBody`) reçoit la totalité de
///   la structure (paragraphes, listes, tableaux) ;
/// - un nœud n'acceptant que de l'inline (Visa, Considérant, libellés...)
///   reçoit le texte et les `Span` mis bout à bout (paragraphes séparés par
///   un saut de ligne), listes et tableaux n'y étant pas représentables ;
/// - un nœud déjà terminal (`Plain` visé directement, par ex. depuis un
///   identifiant renvoyé par `read_structure`) garde l'ancien comportement :
///   remplacement direct de son texte, sans interprétation Markdown.
fn fill_leaf(body: &mut YrsBody, id: BodyNodeId, content: &str) -> anyhow::Result<()> {
    let target = content_target(body, id);
    let target_kind = body.kind_of(target);

    if target_kind.can_accept_child(NodeKind::Paragraphe) {
        let blocks = parse_markdown(body, content);
        for child in body.children_of(target) {
            body.remove_subtree(child)?;
        }
        if blocks.is_empty() {
            let plain = body.create_node(NodeSpec::Plain(String::new()));
            let para = body.create_node(NodeSpec::Paragraphe);
            body.append_child_unchecked(para, plain)?;
            return body.append_child_unchecked(target, para);
        }
        for block in blocks {
            let node = match block {
                MdBlock::Paragraph(children) => {
                    let para = body.create_node(NodeSpec::Paragraphe);
                    for child in children {
                        body.append_child_unchecked(para, child)?;
                    }
                    para
                }
                MdBlock::Node(node) => node,
            };
            body.append_child_unchecked(target, node)?;
        }
        return Ok(());
    }

    if target_kind.can_accept_child(NodeKind::Plain) {
        // Nœud inline uniquement : aplatit les paragraphes (séparés par un
        // saut de ligne littéral) et ignore listes/tableaux, non
        // représentables ici.
        let blocks = parse_markdown(body, content);
        let mut inline = Vec::new();
        for block in blocks {
            if let MdBlock::Paragraph(children) = block {
                if children.is_empty() {
                    continue;
                }
                if !inline.is_empty() {
                    inline.push(body.create_node(NodeSpec::Plain("\n".to_string())));
                }
                inline.extend(children);
            }
        }
        for child in body.children_of(target) {
            body.remove_subtree(child)?;
        }
        if inline.is_empty() {
            inline.push(body.create_node(NodeSpec::Plain(String::new())));
        }
        for child in inline {
            body.append_child_unchecked(target, child)?;
        }
        return Ok(());
    }

    let leaf = if target_kind == NodeKind::Plain {
        target
    } else {
        body.first_leaf_of(target)
    };
    if body.kind_of(leaf) != NodeKind::Plain {
        anyhow::bail!("le nœud {id} n'a pas de contenu textuel modifiable");
    }
    body.set_spec_unchecked(leaf, NodeSpec::Plain(content.to_string()))
}

/// Style d'emphase courant lors de la conversion d'un fragment Markdown en
/// nœuds `Plain`/`Span`, empilé au fil des balises `Strong`/`Emphasis`/
/// `Strikethrough` imbriquées (voir [`parse_inline`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strikeout: bool,
}

/// Bloc de contenu issu du parsing Markdown, avant matérialisation dans
/// l'arbre par [`fill_leaf`] : un paragraphe garde ses enfants `Plain`/
/// `Span` à part (pas encore enveloppés dans un nœud `Paragraphe`) pour que
/// l'appelant puisse choisir de les aplatir (nœud cible n'acceptant que de
/// l'inline) ou de les envelopper (nœud cible acceptant des blocs).
enum MdBlock {
    Paragraph(Vec<BodyNodeId>),
    /// Liste ou tableau déjà entièrement construits (non aplatissables).
    Node(BodyNodeId),
}

type MdEvents<'a> = std::iter::Peekable<std::vec::IntoIter<Event<'a>>>;

/// Parse `markdown` en une séquence de blocs de haut niveau, pour l'outil
/// `fill_section` (voir [`fill_leaf`]).
fn parse_markdown(body: &mut YrsBody, markdown: &str) -> Vec<MdBlock> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let events: Vec<Event> = Parser::new_ext(markdown, options).collect();
    let mut events = events.into_iter().peekable();
    parse_blocks(body, &mut events)
}

/// Consomme les événements de bloc disponibles, en s'arrêtant (sans le
/// consommer) sur le premier `Event::End` qui ne correspond à aucun bloc
/// géré ici : c'est le signal que le conteneur englobant (cellule de
/// tableau, item de liste, citation...) doit reprendre la main.
fn parse_blocks(body: &mut YrsBody, events: &mut MdEvents) -> Vec<MdBlock> {
    let mut blocks = Vec::new();
    while let Some(event) = events.peek().cloned() {
        match event {
            Event::Start(Tag::Paragraph) => {
                events.next();
                let children = parse_inline(body, events);
                if matches!(events.peek(), Some(Event::End(TagEnd::Paragraph))) {
                    events.next();
                }
                blocks.push(MdBlock::Paragraph(children));
            }
            Event::Start(Tag::Heading { .. }) => {
                events.next();
                let children = parse_inline(body, events);
                if matches!(events.peek(), Some(Event::End(TagEnd::Heading(_)))) {
                    events.next();
                }
                blocks.push(MdBlock::Paragraph(children));
            }
            Event::Start(Tag::List(start)) => {
                events.next();
                blocks.push(MdBlock::Node(parse_list(body, events, start)));
            }
            Event::Start(Tag::Table(_)) => {
                events.next();
                blocks.push(MdBlock::Node(parse_table(body, events)));
            }
            Event::Start(Tag::BlockQuote(_)) => {
                events.next();
                blocks.extend(parse_blocks(body, events));
                if matches!(events.peek(), Some(Event::End(TagEnd::BlockQuote(_)))) {
                    events.next();
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                events.next();
                let mut text = String::new();
                while let Some(event) = events.next() {
                    match event {
                        Event::Text(t) => text.push_str(&t),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                blocks.push(MdBlock::Paragraph(vec![
                    body.create_node(NodeSpec::Plain(text)),
                ]));
            }
            // Contenu inline directement au niveau bloc : c'est le cas des
            // cellules de tableau GFM, dont le contenu n'est jamais
            // enveloppé dans un `Paragraph` par pulldown-cmark.
            Event::Text(_)
            | Event::Code(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Start(Tag::Strong)
            | Event::Start(Tag::Emphasis)
            | Event::Start(Tag::Strikethrough)
            | Event::Start(Tag::Link { .. }) => {
                let children = parse_inline(body, events);
                if !children.is_empty() {
                    blocks.push(MdBlock::Paragraph(children));
                }
            }
            Event::End(_) => break,
            _ => {
                events.next();
            }
        }
    }
    blocks
}

/// Consomme les événements inline (texte, gras, italique, barré, liens)
/// disponibles et les matérialise en nœuds `Plain`/`Span`, en fusionnant les
/// suites de texte consécutif de même style en un seul nœud. S'arrête (sans
/// le consommer) sur le premier événement qui n'est pas de l'inline reconnu
/// (fin du bloc englobant, sous-bloc imbriqué...).
fn parse_inline(body: &mut YrsBody, events: &mut MdEvents) -> Vec<BodyNodeId> {
    let mut children = Vec::new();
    let mut style = InlineStyle::default();
    let mut style_stack = Vec::new();
    let mut pending = String::new();

    while let Some(event) = events.peek().cloned() {
        match event {
            Event::Text(text) => {
                pending.push_str(&text);
                events.next();
            }
            Event::Code(text) => {
                pending.push_str(&text);
                events.next();
            }
            Event::SoftBreak => {
                pending.push(' ');
                events.next();
            }
            Event::HardBreak => {
                pending.push('\n');
                events.next();
            }
            Event::Start(Tag::Strong) => {
                flush_pending(body, &mut pending, style, &mut children);
                style_stack.push(style);
                style.bold = true;
                events.next();
            }
            Event::Start(Tag::Emphasis) => {
                flush_pending(body, &mut pending, style, &mut children);
                style_stack.push(style);
                style.italic = true;
                events.next();
            }
            Event::Start(Tag::Strikethrough) => {
                flush_pending(body, &mut pending, style, &mut children);
                style_stack.push(style);
                style.strikeout = true;
                events.next();
            }
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                flush_pending(body, &mut pending, style, &mut children);
                style = style_stack.pop().unwrap_or_default();
                events.next();
            }
            // Le texte du lien est traité comme de l'inline normal ; sa
            // cible n'a pas d'équivalent dans le modèle de contenu.
            Event::Start(Tag::Link { .. }) | Event::End(TagEnd::Link) => {
                events.next();
            }
            _ => break,
        }
    }
    flush_pending(body, &mut pending, style, &mut children);
    children
}

/// Matérialise le texte accumulé dans `pending` en un nœud `Plain` (style
/// par défaut) ou `Span` (sinon), et vide `pending`. Ne fait rien si
/// `pending` est vide (styles ouverts puis refermés sans texte entre les
/// deux, par ex.).
fn flush_pending(
    body: &mut YrsBody,
    pending: &mut String,
    style: InlineStyle,
    out: &mut Vec<BodyNodeId>,
) {
    if pending.is_empty() {
        return;
    }
    let text = std::mem::take(pending);
    if style == InlineStyle::default() {
        out.push(body.create_node(NodeSpec::Plain(text)));
    } else {
        let span = body.create_node(NodeSpec::Span(Span {
            bold: style.bold,
            italic: style.italic,
            underline: false,
            strikeout: style.strikeout,
        }));
        let plain = body.create_node(NodeSpec::Plain(text));
        let _ = body.append_child_unchecked(span, plain);
        out.push(span);
    }
}

/// Construit un nœud `List` à partir des `Item` consécutifs, jusqu'au
/// `TagEnd::List` correspondant (le `Start` a déjà été consommé par
/// l'appelant).
fn parse_list(body: &mut YrsBody, events: &mut MdEvents, start: Option<u64>) -> BodyNodeId {
    let marker = if start.is_some() {
        ListMarker::Decimal
    } else {
        ListMarker::Disc
    };
    let list = body.create_node(NodeSpec::List(List {
        marker,
        start: start.map(|s| s as u32),
    }));

    while let Some(event) = events.peek().cloned() {
        match event {
            Event::Start(Tag::Item) => {
                events.next();
                let children = parse_item(body, events);
                let item = body.create_node(NodeSpec::ListItem(ListItem { marker }));
                for child in children {
                    let _ = body.append_child_unchecked(item, child);
                }
                let _ = body.append_child_unchecked(list, item);
            }
            Event::End(TagEnd::List(_)) => {
                events.next();
                break;
            }
            _ => {
                events.next();
            }
        }
    }
    list
}

/// Contenu inline d'un item de liste : `pulldown-cmark` l'enveloppe dans un
/// `Paragraph` pour une liste « lâche », ou l'émet directement pour une
/// liste « compacte ». Tout contenu de bloc restant (sous-liste, second
/// paragraphe...) est consommé puis ignoré, `ListItem` n'acceptant que du
/// contenu inline (voir [`NodeKind::CHILDREN_TABLE`]).
fn parse_item(body: &mut YrsBody, events: &mut MdEvents) -> Vec<BodyNodeId> {
    let children = if matches!(events.peek(), Some(Event::Start(Tag::Paragraph))) {
        events.next();
        let children = parse_inline(body, events);
        if matches!(events.peek(), Some(Event::End(TagEnd::Paragraph))) {
            events.next();
        }
        children
    } else {
        parse_inline(body, events)
    };

    let mut depth = 0i32;
    while let Some(event) = events.peek().cloned() {
        match event {
            Event::End(TagEnd::Item) if depth == 0 => break,
            Event::Start(_) => {
                depth += 1;
                events.next();
            }
            Event::End(_) => {
                depth -= 1;
                events.next();
            }
            _ => {
                events.next();
            }
        }
    }
    if matches!(events.peek(), Some(Event::End(TagEnd::Item))) {
        events.next();
    }
    children
}

/// Construit un nœud `Table` à partir des lignes (`TableHead`/`TableRow`,
/// traitées de façon identique, ce modèle de contenu ne distinguant pas
/// l'en-tête) rencontrées, jusqu'au `TagEnd::Table` correspondant.
fn parse_table(body: &mut YrsBody, events: &mut MdEvents) -> BodyNodeId {
    let table = body.create_node(NodeSpec::Table);
    while let Some(event) = events.peek().cloned() {
        match event {
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                events.next();
                let row = parse_table_row(body, events);
                let _ = body.append_child_unchecked(table, row);
            }
            Event::End(TagEnd::Table) => {
                events.next();
                break;
            }
            _ => {
                events.next();
            }
        }
    }
    table
}

/// Construit un nœud `TableRow` à partir des `TableCell` rencontrées,
/// jusqu'au `TagEnd::TableHead`/`TagEnd::TableRow` correspondant.
fn parse_table_row(body: &mut YrsBody, events: &mut MdEvents) -> BodyNodeId {
    let row = body.create_node(NodeSpec::TableRow);
    while let Some(event) = events.peek().cloned() {
        match event {
            Event::Start(Tag::TableCell) => {
                events.next();
                let blocks = parse_blocks(body, events);
                if matches!(events.peek(), Some(Event::End(TagEnd::TableCell))) {
                    events.next();
                }
                let cell = body.create_node(NodeSpec::TableCell);
                for block in blocks {
                    match block {
                        MdBlock::Paragraph(children) => {
                            let para = body.create_node(NodeSpec::Paragraphe);
                            for child in children {
                                let _ = body.append_child_unchecked(para, child);
                            }
                            let _ = body.append_child_unchecked(cell, para);
                        }
                        // Une cellule n'accepte que Paragraphe/List (pas de
                        // tableau imbriqué, voir `NodeKind::CHILDREN_TABLE`).
                        MdBlock::Node(node) if body.kind_of(node) == NodeKind::List => {
                            let _ = body.append_child_unchecked(cell, node);
                        }
                        MdBlock::Node(_) => {}
                    }
                }
                let _ = body.append_child_unchecked(row, cell);
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                events.next();
                break;
            }
            _ => {
                events.next();
            }
        }
    }
    row
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

/// Relaie à tous les clients websocket connectés à la salle les réflexions
/// du modèle et les appels d'outils que l'orchestration déclenche (voir
/// [`AgentObserver`]), pour affichage dans `agent::AgentPanel` au fil de
/// l'eau plutôt qu'une fois la tâche entièrement terminée, et lit le contenu
/// des documents précédemment fournis par l'inspecteur (voir
/// [`DocumentContentPort`]).
///
/// Contrairement à l'ancienne implémentation, cette struct ne relaie plus
/// elle-même les questions de l'agent ni n'attend leur réponse en bloquant :
/// une pause HITL est désormais un [`agent::orchestration::RunOutcome::Paused`]
/// traduit en `ServerMessage::Interaction*` par l'appelant (voir
/// `super::ws::spawn_agent_run`), et sa réponse est appliquée à l'état
/// persisté du run (voir `storage::agent_run`) plutôt qu'au travers d'un
/// canal propre à cette connexion — c'est ce qui permet à une pause de
/// survivre à une déconnexion ou un redémarrage du serveur.
///
/// Diffuse sur `agent_events` (voir [`super::state::EditorRoom::agent_events`])
/// plutôt que sur un canal propre à la connexion qui a démarré la tâche : une
/// connexion qui rejoint la salle après coup (nouvel onglet, reconnexion
/// après un rechargement de page) continue ainsi de recevoir la progression
/// d'une tâche déjà en cours, plutôt que de la perdre silencieusement.
pub struct WsUserInteraction {
    agent_events: broadcast::Sender<String>,
    pool: storage::Pool,
}

impl WsUserInteraction {
    #[must_use]
    pub fn new(agent_events: broadcast::Sender<String>, pool: storage::Pool) -> Self {
        Self { agent_events, pool }
    }

    /// Sérialise puis diffuse `message` à tous les pairs connectés à la
    /// salle : silencieux si aucune connexion n'y est actuellement abonnée
    /// (voir `tokio::sync::broadcast::Sender::send`) ou si la sérialisation
    /// échoue (ne devrait pas arriver), ce qui est sans conséquence puisque
    /// l'état de l'orchestration reste de toute façon persisté (voir
    /// `storage::agent_run`) et rejoué à la prochaine connexion (voir
    /// `super::ws::restore_active_session`/`replay_pending_interaction`).
    fn broadcast(&self, message: &ServerMessage) {
        if let Ok(text) = serde_json::to_string(message) {
            let _ = self.agent_events.send(text);
        }
    }
}

#[async_trait::async_trait]
impl DocumentContentPort for WsUserInteraction {
    async fn fetch_content(&self, document_id: &str) -> Result<DocumentContent, ToolError> {
        let id: ID = document_id.parse().map_err(|_| {
            ToolError::InvalidArguments(format!("identifiant de document invalide : {document_id}"))
        })?;
        let document = storage::agent_run::fetch_document(&self.pool, &id)
            .await
            .map_err(|_| {
                ToolError::InvalidArguments(format!("document introuvable : {document_id}"))
            })?;
        Ok(DocumentContent {
            bytes: document.bytes,
            mime_type: document.mime_type,
            file_name: document.file_name,
        })
    }
}

#[async_trait::async_trait]
impl AgentObserver for WsUserInteraction {
    async fn on_reasoning_delta(&self, agent_label: &str, delta: &str) {
        self.broadcast(&ServerMessage::AgentReasoningDelta {
            agent_label: agent_label.to_string(),
            delta: delta.to_string(),
        });
    }

    async fn on_content_delta(&self, agent_label: &str, delta: &str) {
        self.broadcast(&ServerMessage::AgentContentDelta {
            agent_label: agent_label.to_string(),
            delta: delta.to_string(),
        });
    }

    async fn on_turn_finished(&self, _agent_label: &str) {
        self.broadcast(&ServerMessage::AgentStepFinished);
    }

    async fn on_tool_call_started(&self, agent_label: &str, call: &ToolCall) {
        self.broadcast(&ServerMessage::AgentToolCallStarted {
            agent_label: agent_label.to_string(),
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        });
    }

    async fn on_tool_call_finished(
        &self,
        agent_label: &str,
        call_id: &str,
        result: &Result<String, String>,
    ) {
        let (ok, output) = match result {
            Ok(output) => (true, output.clone()),
            Err(message) => (false, message.clone()),
        };
        self.broadcast(&ServerMessage::AgentToolCallFinished {
            agent_label: agent_label.to_string(),
            id: call_id.to_string(),
            ok,
            output,
        });
    }
}

/// Implémentation de [`agent::catalog::AgentCatalog`] adossée à la table
/// `agent_profiles` (voir `storage::agent_profile`) : le catalogue d'experts
/// que le Superviseur peut instancier à la volée est une donnée
/// administrable (`/admin/agent-profiles`), jamais une struct Rust par
/// expert.
pub struct StorageAgentCatalog {
    pool: storage::Pool,
}

impl StorageAgentCatalog {
    #[must_use]
    pub fn new(pool: storage::Pool) -> Self {
        Self { pool }
    }
}

fn to_catalog_profile(stored: shared::model::AgentProfile) -> agent::AgentProfile {
    agent::AgentProfile {
        id: stored.name,
        display_name: stored.display_name,
        system_prompt: stored.system_prompt,
        tool_names: stored.tool_names,
        max_steps: u32::try_from(stored.max_steps).unwrap_or(1).max(1),
    }
}

#[async_trait::async_trait]
impl agent::AgentCatalog for StorageAgentCatalog {
    async fn list(&self) -> Result<Vec<agent::AgentProfile>, ToolError> {
        let profiles = storage::agent_profile::list_enabled_agent_profiles(&self.pool)
            .await
            .map_err(|error| ToolError::Other(error.to_string()))?;
        Ok(profiles.into_iter().map(to_catalog_profile).collect())
    }

    async fn get(&self, id: &str) -> Result<Option<agent::AgentProfile>, ToolError> {
        let profile = storage::agent_profile::get_enabled_agent_profile_by_name(&self.pool, id)
            .await
            .map_err(|error| ToolError::Other(error.to_string()))?;
        Ok(profile.map(to_catalog_profile))
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
        (
            EditorRoom::new(test_pool(), None, body, 1, legal_act::YrsReview::new(), 1),
            article,
        )
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

    /// Concatène le texte de tous les descendants `Plain` de `id`, pour
    /// vérifier le texte rendu d'un nœud sans se soucier de son
    /// enveloppement éventuel dans des `Span` (`BodyRead::text_of` ne
    /// renvoie du texte que pour un nœud `Plain` pris isolément).
    fn text_of_subtree(body: &YrsBody, id: BodyNodeId) -> String {
        if body.kind_of(id) == NodeKind::Plain {
            return body.text_of(id);
        }
        body.children_of(id)
            .into_iter()
            .map(|child| text_of_subtree(body, child))
            .collect()
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
    async fn fill_section_converts_bold_and_italic_to_spans() {
        let (room, article) = room_with_article();
        let editor = new_editor(&room);

        editor
            .fill_section(
                &article.to_string(),
                "Du texte **en gras** et *en italique*.",
            )
            .await
            .unwrap();

        let body = room.body.lock().await;
        let article_body = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
            .unwrap();
        let para = body.first_child_of(article_body).unwrap();
        assert_eq!(body.kind_of(para), NodeKind::Paragraphe);

        let children = body.children_of(para);
        let full_text: String = children
            .iter()
            .map(|&c| text_of_subtree(&body, c))
            .collect();
        assert_eq!(full_text, "Du texte en gras et en italique.");

        let bold_span = children
            .iter()
            .find(|&&c| body.kind_of(c) == NodeKind::Span && text_of_subtree(&body, c) == "en gras")
            .expect("span en gras introuvable");
        if let NodeSpec::Span(span) = body.spec_of(*bold_span) {
            assert!(span.bold);
            assert!(!span.italic);
        } else {
            panic!("attendu un NodeSpec::Span");
        }

        let italic_span = children
            .iter()
            .find(|&&c| {
                body.kind_of(c) == NodeKind::Span && text_of_subtree(&body, c) == "en italique"
            })
            .expect("span en italique introuvable");
        if let NodeSpec::Span(span) = body.spec_of(*italic_span) {
            assert!(span.italic);
            assert!(!span.bold);
        } else {
            panic!("attendu un NodeSpec::Span");
        }
    }

    #[tokio::test]
    async fn fill_section_creates_one_paragraphe_per_markdown_paragraph() {
        let (room, article) = room_with_article();
        let editor = new_editor(&room);

        editor
            .fill_section(
                &article.to_string(),
                "Premier paragraphe.\n\nSecond paragraphe.",
            )
            .await
            .unwrap();

        let body = room.body.lock().await;
        let article_body = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
            .unwrap();
        let paragraphs = body.children_of(article_body);
        assert_eq!(paragraphs.len(), 2);
        assert!(
            paragraphs
                .iter()
                .all(|&p| body.kind_of(p) == NodeKind::Paragraphe)
        );
        assert_eq!(
            body.text_of(body.first_leaf_of(paragraphs[0])),
            "Premier paragraphe."
        );
        assert_eq!(
            body.text_of(body.first_leaf_of(paragraphs[1])),
            "Second paragraphe."
        );
    }

    #[tokio::test]
    async fn fill_section_creates_a_bullet_list() {
        let (room, article) = room_with_article();
        let editor = new_editor(&room);

        editor
            .fill_section(&article.to_string(), "- Premier item\n- Second item")
            .await
            .unwrap();

        let body = room.body.lock().await;
        let article_body = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
            .unwrap();
        let list = body.first_child_of(article_body).unwrap();
        assert_eq!(body.kind_of(list), NodeKind::List);

        let items = body.children_of(list);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|&i| body.kind_of(i) == NodeKind::ListItem));
        assert_eq!(body.text_of(body.first_leaf_of(items[0])), "Premier item");
        assert_eq!(body.text_of(body.first_leaf_of(items[1])), "Second item");
    }

    #[tokio::test]
    async fn fill_section_creates_a_table() {
        let (room, article) = room_with_article();
        let editor = new_editor(&room);

        editor
            .fill_section(&article.to_string(), "| A | B |\n| --- | --- |\n| 1 | 2 |")
            .await
            .unwrap();

        let body = room.body.lock().await;
        let article_body = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::ArticleBody)
            .unwrap();
        let table = body.first_child_of(article_body).unwrap();
        assert_eq!(body.kind_of(table), NodeKind::Table);

        let rows = body.children_of(table);
        assert_eq!(rows.len(), 2);
        for &row in &rows {
            assert_eq!(body.kind_of(row), NodeKind::TableRow);
            let cells = body.children_of(row);
            assert_eq!(cells.len(), 2);
            for &cell in &cells {
                assert_eq!(body.kind_of(cell), NodeKind::TableCell);
            }
        }
        let header_texts: Vec<String> = body
            .children_of(rows[0])
            .into_iter()
            .map(|c| body.text_of(body.first_leaf_of(c)))
            .collect();
        assert_eq!(header_texts, vec!["A".to_string(), "B".to_string()]);
    }

    #[tokio::test]
    async fn fill_section_on_inline_only_node_flattens_paragraphs_into_spans() {
        let (room, _) = room_with_article();
        let editor = new_editor(&room);
        let root = { room.body.lock().await.root() };

        let visa = {
            let mut body = room.body.lock().await;
            body.append_node(root, NodeSpec::Visa).unwrap()
        };

        editor
            .fill_section(&visa.to_string(), "Vu le **code** de l'environnement.")
            .await
            .unwrap();

        let body = room.body.lock().await;
        let children = body.children_of(visa);
        assert!(
            children
                .iter()
                .all(|&c| matches!(body.kind_of(c), NodeKind::Plain | NodeKind::Span))
        );
        let full_text: String = children
            .iter()
            .map(|&c| text_of_subtree(&body, c))
            .collect();
        assert_eq!(full_text, "Vu le code de l'environnement.");
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
