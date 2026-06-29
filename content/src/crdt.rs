use anyhow::bail;
use yrs::{Any, Array, ArrayPrelim, Doc, GetString, Map, MapPrelim, Out, ReadTxn, Text, TextPrelim, TransactionMut, Transact};

use crate::{Cell, ContentId, ContentKind, ContentRead, ContentWrite, List, ListItem, ListMarker, NodeSpec, Paragraph, Row, Span, Table};

/// Backend "mode Yrs" : l'arbre de contenu est porté par un [`yrs::Doc`] et
/// peut donc être synchronisé entre plusieurs pairs via CRDT.
///
/// Chaque noeud est représenté par une [`yrs::MapRef`] indexée par
/// [`ContentId`] (converti en chaîne) dans la map racine `nodes`, ce qui en
/// fait des entrées découvrables par tout pair qui reçoit les mises à jour
/// du document.
pub struct YrsContent {
    doc: Doc,
    nodes: yrs::MapRef,
    root: ContentId,
}

impl YrsContent {
    /// Initialise un noeud `Content` vierge dans `content`, qu'il s'agisse
    /// de la racine d'un `Doc` ou d'une `MapRef` imbriquée dans une
    /// structure plus large (ex: le champ `content` d'un `LegalAct`).
    ///
    /// `content` doit être une map fraîchement créée (vide) : `init`
    /// n'écrase pas un noeud déjà initialisé, voir [`YrsContent::open`] pour
    /// ce cas.
    pub fn init(doc: Doc, content: yrs::MapRef) -> Self {
        let root = ContentId::new();

        let mut txn = doc.transact_mut();
        let nodes = content.insert(&mut txn, "nodes", MapPrelim::default());
        content.insert(&mut txn, "root", root.to_string());
        nodes.insert(&mut txn, root.to_string(), node_prelim(&NodeSpec::Root, None));
        drop(txn);

        Self { doc, nodes, root }
    }

    /// Crée un nouveau document Yrs autonome, avec un unique noeud `Content`
    /// racine.
    pub fn new() -> Self {
        let doc = Doc::new();
        let content = doc.get_or_insert_map("content");
        Self::init(doc, content)
    }

    /// Reconstruit le handle à partir d'un noeud `Content` déjà initialisé
    /// (typiquement rejoint depuis un pair distant après synchronisation),
    /// que `content` soit la racine du `Doc` ou une `MapRef` imbriquée dans
    /// une structure plus large (ex: le champ `content` d'un `LegalAct`).
    pub fn open(doc: Doc, content: yrs::MapRef) -> anyhow::Result<Self> {
        let txn = doc.transact();

        let Some(Out::Any(Any::String(root_str))) = content.get(&txn, "root") else {
            bail!("noeud 'content' yrs invalide : champ 'root' manquant ou invalide");
        };
        let root: ContentId = root_str.parse()?;

        let Some(Out::YMap(nodes)) = content.get(&txn, "nodes") else {
            bail!("noeud 'content' yrs invalide : champ 'nodes' manquant ou invalide");
        };

        drop(txn);
        Ok(Self { doc, nodes, root })
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    fn node_map(&self, txn: &impl ReadTxn, id: ContentId) -> Option<yrs::MapRef> {
        match self.nodes.get(txn, &id.to_string()) {
            Some(Out::YMap(map)) => Some(map),
            _ => None,
        }
    }

    fn children_array(&self, txn: &impl ReadTxn, id: ContentId) -> Option<yrs::ArrayRef> {
        match self.node_map(txn, id)?.get(txn, "children") {
            Some(Out::YArray(array)) => Some(array),
            _ => None,
        }
    }
}

impl Default for YrsContent {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentRead for YrsContent {
    fn root(&self) -> ContentId {
        self.root
    }

    fn kind_of(&self, id: ContentId) -> ContentKind {
        let txn = self.doc.transact();
        let node = self.node_map(&txn, id).unwrap_or_else(|| panic!("noeud de contenu inconnu : {id}"));
        read_kind(&node, &txn)
    }

    fn text_of(&self, id: ContentId) -> String {
        let txn = self.doc.transact();
        let Some(node) = self.node_map(&txn, id) else { return String::new() };

        match node.get(&txn, "text") {
            Some(Out::YText(text)) => text.get_string(&txn),
            _ => String::new(),
        }
    }

    fn parent_of(&self, id: ContentId) -> Option<ContentId> {
        let txn = self.doc.transact();
        let node = self.node_map(&txn, id)?;

        match node.get(&txn, "parent") {
            Some(Out::Any(Any::String(parent_str))) => parent_str.parse().ok(),
            _ => None,
        }
    }

    fn children_of(&self, id: ContentId) -> Vec<ContentId> {
        let txn = self.doc.transact();
        let Some(children) = self.children_array(&txn, id) else { return vec![] };

        children
            .iter(&txn)
            .filter_map(|out| match out {
                Out::Any(Any::String(id_str)) => id_str.parse().ok(),
                _ => None,
            })
            .collect()
    }

    fn spec_of(&self, id: ContentId) -> NodeSpec {
        let txn = self.doc.transact();
        let node = self.node_map(&txn, id).unwrap_or_else(|| panic!("noeud de contenu inconnu : {id}"));
        read_spec(&node, &txn)
    }
}

impl ContentWrite for YrsContent {
    fn create_node<N>(&mut self, spec: N) -> ContentId
    where
        NodeSpec: From<N>,
    {
        let spec = NodeSpec::from(spec);
        let id = ContentId::new();

        let mut txn = self.doc.transact_mut();
        self.nodes.insert(&mut txn, id.to_string(), node_prelim(&spec, None));

        id
    }

    fn insert_child_at(&mut self, parent: ContentId, index: usize, child: ContentId) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();

        let Some(parent_map) = self.node_map(&txn, parent) else {
            bail!("noeud de contenu inconnu : {parent}")
        };
        let Some(child_map) = self.node_map(&txn, child) else {
            bail!("noeud de contenu inconnu : {child}")
        };
        let Some(Out::YArray(children)) = parent_map.get(&txn, "children") else {
            bail!("champ 'children' invalide pour le noeud {parent}")
        };

        children.insert(&mut txn, index as u32, child.to_string());
        child_map.insert(&mut txn, "parent", parent.to_string());

        Ok(())
    }

    fn detach_unchecked(&mut self, id: ContentId) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();

        let Some(node) = self.node_map(&txn, id) else {
            bail!("noeud de contenu inconnu : {id}")
        };

        let parent_id: Option<ContentId> = match node.get(&txn, "parent") {
            Some(Out::Any(Any::String(parent_str))) => parent_str.parse().ok(),
            _ => None,
        };

        if let Some(parent_id) = parent_id
            && let Some(parent_map) = self.node_map(&txn, parent_id)
            && let Some(Out::YArray(children)) = parent_map.get(&txn, "children")
        {
            let index = children.iter(&txn).position(|out| match out {
                Out::Any(Any::String(id_str)) => id_str.parse::<ContentId>().ok() == Some(id),
                _ => false,
            });

            if let Some(index) = index {
                children.remove(&mut txn, index as u32);
            }
        }

        node.insert(&mut txn, "parent", yrs::In::Any(Any::Null));

        Ok(())
    }

    fn remove_node(&mut self, id: ContentId) -> anyhow::Result<()> {
        self.detach_unchecked(id)?;

        let mut txn = self.doc.transact_mut();
        let mut subtree = vec![];
        collect_subtree(&self.nodes, &txn, id, &mut subtree);

        for descendant in subtree {
            self.nodes.remove(&mut txn, &descendant.to_string());
        }

        Ok(())
    }

    fn insert_text(&mut self, id: ContentId, char_index: usize, value: &str) {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else { return };
        let Some(Out::YText(text)) = node.get(&txn, "text") else { return };

        let byte_index = byte_index_of(&text.get_string(&txn), char_index);
        text.insert(&mut txn, byte_index as u32, value);
    }

    fn remove_text(&mut self, id: ContentId, char_index: usize, char_count: usize) {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else { return };
        let Some(Out::YText(text)) = node.get(&txn, "text") else { return };

        let current = text.get_string(&txn);
        let mut byte_indices = current.char_indices().map(|(i, _)| i);
        let Some(start) = byte_indices.nth(char_index) else { return };
        let end = byte_indices.nth(char_count.saturating_sub(1)).unwrap_or(current.len());

        text.remove_range(&mut txn, start as u32, (end - start) as u32);
    }

    fn set_spec(&mut self, id: ContentId, spec: NodeSpec) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else {
            bail!("noeud de contenu inconnu : {id}")
        };

        let current_kind = read_kind(&node, &txn);
        let new_kind = spec.kind();

        if new_kind != current_kind {
            bail!("impossible de remplacer un noeud {current_kind} par un noeud {new_kind} : les genres diffèrent");
        }

        write_spec(&node, &mut txn, &spec);
        Ok(())
    }
}

fn byte_index_of(text: &str, char_index: usize) -> usize {
    text.char_indices().nth(char_index).map(|(i, _)| i).unwrap_or(text.len())
}

/// Collecte `id` et tous ses descendants (parcours préfixe) dans `out`.
fn collect_subtree(nodes: &yrs::MapRef, txn: &impl ReadTxn, id: ContentId, out: &mut Vec<ContentId>) {
    out.push(id);

    let Some(Out::YMap(node)) = nodes.get(txn, &id.to_string()) else { return };
    let Some(Out::YArray(children)) = node.get(txn, "children") else { return };

    let child_ids = children
        .iter(txn)
        .filter_map(|out| match out {
            Out::Any(Any::String(id_str)) => id_str.parse::<ContentId>().ok(),
            _ => None,
        })
        .collect::<Vec<_>>();

    for child in child_ids {
        collect_subtree(nodes, txn, child, out);
    }
}

fn read_kind(node: &yrs::MapRef, txn: &impl ReadTxn) -> ContentKind {
    let Some(Out::Any(Any::String(kind_str))) = node.get(txn, "kind") else {
        panic!("champ 'kind' invalide pour un noeud yrs")
    };

    kind_str.parse().unwrap_or_else(|_| panic!("kind de noeud invalide : {kind_str}"))
}

fn read_bool(node: &yrs::MapRef, txn: &impl ReadTxn, key: &str) -> bool {
    matches!(node.get(txn, key), Some(Out::Any(Any::Bool(true))))
}

fn read_marker(node: &yrs::MapRef, txn: &impl ReadTxn, key: &str) -> ListMarker {
    match node.get(txn, key) {
        Some(Out::Any(Any::String(marker_str))) => marker_str.parse().unwrap_or_default(),
        _ => ListMarker::default(),
    }
}

fn read_u32(node: &yrs::MapRef, txn: &impl ReadTxn, key: &str) -> Option<u32> {
    match node.get(txn, key) {
        Some(Out::Any(Any::Number(n))) => Some(n as u32),
        _ => None,
    }
}

/// Reconstruit la spécification complète d'un noeud à partir de sa
/// représentation Yrs.
fn read_spec(node: &yrs::MapRef, txn: &impl ReadTxn) -> NodeSpec {
    match read_kind(node, txn) {
        ContentKind::Root => NodeSpec::Root,
        ContentKind::Paragraph => Paragraph.into(),
        ContentKind::Plain => NodeSpec::Plain(match node.get(txn, "text") {
            Some(Out::YText(text)) => text.get_string(txn),
            _ => String::new(),
        }),
        ContentKind::Span => Span {
            bold: read_bool(node, txn, "bold"),
            italic: read_bool(node, txn, "italic"),
            underline: read_bool(node, txn, "underline"),
            strikeout: read_bool(node, txn, "strikeout"),
        }
        .into(),
        ContentKind::List => List {
            marker: read_marker(node, txn, "marker"),
            start: read_u32(node, txn, "start"),
        }
        .into(),
        ContentKind::ListItem => ListItem { marker: read_marker(node, txn, "marker") }.into(),
        ContentKind::Table => Table.into(),
        ContentKind::Row => Row.into(),
        ContentKind::Cell => Cell.into(),
    }
}

/// Met à jour les attributs d'un noeud existant pour correspondre à `spec`,
/// sans toucher à sa position dans l'arbre (parent, enfants). `spec` doit
/// être du même [`ContentKind`] que le noeud (vérifié par l'appelant).
fn write_spec(node: &yrs::MapRef, txn: &mut TransactionMut, spec: &NodeSpec) {
    match spec {
        NodeSpec::Plain(text) => {
            if let Some(Out::YText(existing)) = node.get(txn, "text") {
                let len = existing.len(txn);
                existing.remove_range(txn, 0, len);
                existing.push(txn, text);
            } else {
                node.insert(txn, "text", TextPrelim::new(text.clone()));
            }
        }
        NodeSpec::Span(span) => {
            node.insert(txn, "bold", span.bold);
            node.insert(txn, "italic", span.italic);
            node.insert(txn, "underline", span.underline);
            node.insert(txn, "strikeout", span.strikeout);
        }
        NodeSpec::List(list) => {
            node.insert(txn, "marker", list.marker.as_ref());
            node.insert(txn, "start", list.start.map(yrs::In::from).unwrap_or(yrs::In::Any(Any::Null)));
        }
        NodeSpec::ListItem(item) => {
            node.insert(txn, "marker", item.marker.as_ref());
        }
        NodeSpec::Root | NodeSpec::Paragraph(_) | NodeSpec::Table(_) | NodeSpec::Row(_) | NodeSpec::Cell(_) => {}
    }
}

/// Sérialise une spécification de noeud en valeur Yrs prête à être insérée
/// dans la map `nodes` d'un [`YrsContent`].
fn node_prelim(spec: &NodeSpec, parent: Option<ContentId>) -> MapPrelim {
    let mut fields: Vec<(&str, yrs::In)> = vec![
        ("kind", yrs::In::from(spec.kind().as_ref())),
        (
            "parent",
            parent.map(|p| yrs::In::from(p.to_string())).unwrap_or(yrs::In::Any(Any::Null)),
        ),
        ("children", yrs::In::from(ArrayPrelim::default())),
    ];

    match spec {
        NodeSpec::Plain(text) => fields.push(("text", yrs::In::from(TextPrelim::new(text.clone())))),
        NodeSpec::Span(span) => {
            fields.push(("bold", yrs::In::from(span.bold)));
            fields.push(("italic", yrs::In::from(span.italic)));
            fields.push(("underline", yrs::In::from(span.underline)));
            fields.push(("strikeout", yrs::In::from(span.strikeout)));
        }
        NodeSpec::List(list) => {
            fields.push(("marker", yrs::In::from(list.marker.as_ref())));
            fields.push((
                "start",
                list.start.map(yrs::In::from).unwrap_or(yrs::In::Any(Any::Null)),
            ));
        }
        NodeSpec::ListItem(item) => {
            fields.push(("marker", yrs::In::from(item.marker.as_ref())));
        }
        NodeSpec::Root | NodeSpec::Paragraph(_) | NodeSpec::Table(_) | NodeSpec::Row(_) | NodeSpec::Cell(_) => {}
    }

    MapPrelim::from_iter(fields)
}

#[cfg(test)]
mod tests {
    use crate::{Cell, ContentKind as Kind, List, ListItem, Row, Span, Table};

    use super::*;

    #[test]
    fn test_create_compatible_node() {
        let mut body = YrsContent::new();
        let id = body.append_content(body.root(), "").unwrap();
        let got = body.ancestors_of(id).into_iter().map(|id| body.kind_of(id)).collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Paragraph, Kind::Root]);

        let mut body = YrsContent::new();
        let id = body.append_content(body.root(), Cell).unwrap();
        let got = body.ancestors_of(id).into_iter().map(|id| body.kind_of(id)).collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Row, Kind::Table, Kind::Root]);

        let _ = (List::default(), ListItem::default(), Row, Table, Span::default());
    }

    #[test]
    fn test_only_plain_leafs() {
        let mut body = YrsContent::new();
        body.append_content(body.root(), Span::default()).unwrap();
        let id = body.first_leaf_of(body.root());
        let got = body.ancestors_of(id).into_iter().map(|id| body.kind_of(id)).collect::<Vec<_>>();

        assert_eq!(body.kind_of(id), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::Span, Kind::Paragraph, Kind::Root]);
    }

    #[test]
    fn test_text_editing() {
        let mut body = YrsContent::new();
        let id = body.append_content(body.root(), "hello").unwrap();

        body.insert_text(id, 5, " world");
        assert_eq!(body.text_of(id), "hello world");

        body.remove_text(id, 0, 6);
        assert_eq!(body.text_of(id), "world");
    }

    #[test]
    fn test_leaf_navigation() {
        let mut body = YrsContent::new();
        let first = body.append_content(body.root(), "a").unwrap();
        let second = body.append_content(body.root(), "b").unwrap();

        assert_eq!(body.next_leaf_of(first), Some(second));
        assert_eq!(body.prev_leaf_of(second), Some(first));
        assert_eq!(body.leaf_order_of(first, second), Some(std::cmp::Ordering::Less));
    }

    #[test]
    fn test_spec_of_and_set_spec() {
        let mut body = YrsContent::new();
        let id = body.create_node(Span { bold: true, ..Span::default() });

        let NodeSpec::Span(span) = body.spec_of(id) else { panic!("attendu un Span") };
        assert!(span.bold);
        assert!(!span.italic);

        body.set_spec(id, NodeSpec::Span(Span { italic: true, ..Span::default() })).unwrap();
        let NodeSpec::Span(span) = body.spec_of(id) else { panic!("attendu un Span") };
        assert!(!span.bold);
        assert!(span.italic);

        let err = body.set_spec(id, NodeSpec::Plain("x".into())).unwrap_err();
        assert!(err.to_string().contains("genres diffèrent"));
    }

    #[test]
    fn test_merge_with_prev_text() {
        let mut body = YrsContent::new();
        let paragraph = body.create_node(Paragraph);
        body.insert_child_at(body.root(), 0, paragraph).unwrap();

        let first = body.create_node("hello ");
        let second = body.create_node("world");
        body.insert_child_at(paragraph, 0, first).unwrap();
        body.insert_child_at(paragraph, 1, second).unwrap();

        let merged = body.merge_with_prev(second).unwrap();
        assert_eq!(merged, first);
        assert_eq!(body.text_of(first), "hello world");
        assert_eq!(body.children_of(paragraph), vec![first]);
    }

    #[test]
    fn test_merge_with_prev_kind_mismatch() {
        let mut body = YrsContent::new();
        let paragraph = body.create_node(Paragraph);
        body.insert_child_at(body.root(), 0, paragraph).unwrap();

        let plain = body.create_node("x");
        let span = body.create_node(Span::default());
        body.insert_child_at(paragraph, 0, plain).unwrap();
        body.insert_child_at(paragraph, 1, span).unwrap();

        let err = body.merge_with_prev(span).unwrap_err();
        assert!(err.to_string().contains("structures incompatibles"));
    }

    #[test]
    fn test_merge_into_paragraph_and_list_item() {
        let mut body = YrsContent::new();
        let paragraph = body.create_node(Paragraph);
        let list_item = body.create_node(ListItem::default());
        body.insert_child_at(body.root(), 0, paragraph).unwrap();
        body.insert_child_at(body.root(), 1, list_item).unwrap();

        let plain = body.create_node("hello");
        let span = body.create_node(Span::default());
        body.insert_child_at(list_item, 0, plain).unwrap();
        body.insert_child_at(list_item, 1, span).unwrap();

        body.merge_into(paragraph, list_item).unwrap();

        assert_eq!(body.kind_of(paragraph), Kind::Paragraph);
        assert_eq!(body.children_of(paragraph), vec![plain, span]);
        assert_eq!(body.children_of(body.root()), vec![paragraph]);
    }

    #[test]
    fn test_split_node_text() {
        let mut body = YrsContent::new();
        let id = body.append_content(body.root(), "hello world").unwrap();
        let paragraph = body.parent_of(id).unwrap();

        let tail = body.split_node(id, 5).unwrap();
        assert_eq!(body.text_of(id), "hello");
        assert_eq!(body.text_of(tail), " world");
        assert_eq!(body.children_of(paragraph), vec![id, tail]);
    }

    #[test]
    fn test_split_node_container() {
        let mut body = YrsContent::new();
        let paragraph = body.create_node(Paragraph);
        body.insert_child_at(body.root(), 0, paragraph).unwrap();

        let a = body.create_node("a");
        let b = body.create_node("b");
        let c = body.create_node("c");
        body.insert_child_at(paragraph, 0, a).unwrap();
        body.insert_child_at(paragraph, 1, b).unwrap();
        body.insert_child_at(paragraph, 2, c).unwrap();

        let new_paragraph = body.split_node(paragraph, 1).unwrap();
        assert_eq!(body.children_of(paragraph), vec![a]);
        assert_eq!(body.children_of(new_paragraph), vec![b, c]);
        assert_eq!(body.children_of(body.root()), vec![paragraph, new_paragraph]);
        assert_eq!(body.kind_of(new_paragraph), Kind::Paragraph);
    }

    #[test]
    fn test_open_from_synced_doc() {
        use yrs::updates::decoder::Decode;

        let mut writer = YrsContent::new();
        writer.append_content(writer.root(), "hello").unwrap();

        // Simule un pair distant qui rejoint le document après synchronisation.
        let update = writer.doc().transact().encode_diff_v1(&yrs::StateVector::default());
        let remote_doc = Doc::new();
        remote_doc
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&update).unwrap())
            .unwrap();

        let remote_content = remote_doc.get_or_insert_map("content");
        let reader = YrsContent::open(remote_doc, remote_content).unwrap();
        let leaf = reader.first_leaf_of(reader.root());
        assert_eq!(reader.text_of(leaf), "hello");
    }
}
