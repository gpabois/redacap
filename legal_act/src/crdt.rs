use anyhow::bail;
use yrs::{Any, Array, ArrayPrelim, Doc, GetString, Map, MapPrelim, Out, ReadTxn, Text, TextPrelim, Transact, TransactionMut};

use crate::{BodyNodeId, NodeKind, NodeSpec};
use crate::traits::node::{BodyRead, BodyWrite};
use content::ListMarker;

/// Backend "mode Yrs" : le corps de l'acte légal est porté par un
/// [`yrs::Doc`] et peut être synchronisé entre plusieurs pairs via CRDT.
///
/// Chaque nœud est représenté par une [`yrs::MapRef`] indexée par
/// `BodyNodeId` (converti en chaîne) dans la map racine `nodes`.
pub struct YrsBody {
    doc: Doc,
    body: yrs::MapRef,
    nodes: yrs::MapRef,
    root: BodyNodeId,
}

impl YrsBody {
    pub fn new() -> Self {
        let doc = Doc::new();
        let body_map = doc.get_or_insert_map("body");
        Self::init(doc, body_map)
    }

    pub fn init(doc: Doc, body: yrs::MapRef) -> Self {
        let root = BodyNodeId::new();
        let mut txn = doc.transact_mut();
        let nodes = body.insert(&mut txn, "nodes", MapPrelim::default());
        body.insert(&mut txn, "root", root.to_string());
        body.insert(&mut txn, "title", "");
        nodes.insert(&mut txn, root.to_string(), node_prelim(&NodeSpec::Root, None));
        drop(txn);
        Self { doc, body, nodes, root }
    }

    pub fn open(doc: Doc, body: yrs::MapRef) -> anyhow::Result<Self> {
        let txn = doc.transact();
        let Some(Out::Any(Any::String(root_str))) = body.get(&txn, "root") else {
            bail!("champ 'root' manquant ou invalide dans le nœud body yrs");
        };
        let root: BodyNodeId = root_str.parse()?;
        let Some(Out::YMap(nodes)) = body.get(&txn, "nodes") else {
            bail!("champ 'nodes' manquant ou invalide dans le nœud body yrs");
        };
        drop(txn);
        Ok(Self { doc, body, nodes, root })
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    fn node_map(&self, txn: &impl ReadTxn, id: BodyNodeId) -> Option<yrs::MapRef> {
        match self.nodes.get(txn, &id.to_string()) {
            Some(Out::YMap(m)) => Some(m),
            _ => None,
        }
    }

    fn children_array(&self, txn: &impl ReadTxn, id: BodyNodeId) -> Option<yrs::ArrayRef> {
        match self.node_map(txn, id)?.get(txn, "children") {
            Some(Out::YArray(a)) => Some(a),
            _ => None,
        }
    }
}

impl Default for YrsBody {
    fn default() -> Self {
        Self::new()
    }
}

impl BodyRead for YrsBody {
    fn root(&self) -> BodyNodeId {
        self.root
    }

    fn kind_of(&self, id: BodyNodeId) -> NodeKind {
        let txn = self.doc.transact();
        let node = self.node_map(&txn, id).unwrap_or_else(|| panic!("nœud inconnu : {id}"));
        read_kind(&node, &txn)
    }

    fn text_of(&self, id: BodyNodeId) -> String {
        let txn = self.doc.transact();
        let Some(node) = self.node_map(&txn, id) else { return String::new() };
        match node.get(&txn, "text") {
            Some(Out::YText(t)) => t.get_string(&txn),
            _ => String::new(),
        }
    }

    fn parent_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        let txn = self.doc.transact();
        let node = self.node_map(&txn, id)?;
        match node.get(&txn, "parent") {
            Some(Out::Any(Any::String(s))) => s.parse().ok(),
            _ => None,
        }
    }

    fn children_of(&self, id: BodyNodeId) -> Vec<BodyNodeId> {
        let txn = self.doc.transact();
        let Some(arr) = self.children_array(&txn, id) else { return vec![] };
        arr.iter(&txn)
            .filter_map(|out| match out {
                Out::Any(Any::String(s)) => s.parse().ok(),
                _ => None,
            })
            .collect()
    }

    fn spec_of(&self, id: BodyNodeId) -> NodeSpec {
        let txn = self.doc.transact();
        let node = self.node_map(&txn, id).unwrap_or_else(|| panic!("nœud inconnu : {id}"));
        read_spec(&node, &txn)
    }

    fn title(&self) -> String {
        let txn = self.doc.transact();
        match self.body.get(&txn, "title") {
            Some(Out::Any(Any::String(s))) => s.to_string(),
            _ => String::new(),
        }
    }
}

impl BodyWrite for YrsBody {
    fn create_node(&mut self, spec: NodeSpec) -> BodyNodeId {
        let id = BodyNodeId::new();
        let mut txn = self.doc.transact_mut();
        self.nodes.insert(&mut txn, id.to_string(), node_prelim(&spec, None));
        id
    }

    fn insert_child_at_unchecked(
        &mut self,
        parent: BodyNodeId,
        index: usize,
        child: BodyNodeId,
    ) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let Some(parent_map) = self.node_map(&txn, parent) else {
            bail!("nœud inconnu : {parent}")
        };
        let Some(child_map) = self.node_map(&txn, child) else {
            bail!("nœud inconnu : {child}")
        };
        let Some(Out::YArray(children)) = parent_map.get(&txn, "children") else {
            bail!("champ 'children' invalide pour le nœud {parent}")
        };
        children.insert(&mut txn, index as u32, child.to_string());
        child_map.insert(&mut txn, "parent", parent.to_string());
        Ok(())
    }

    fn detach_unchecked(&mut self, id: BodyNodeId) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else {
            bail!("nœud inconnu : {id}")
        };
        let parent_id: Option<BodyNodeId> = match node.get(&txn, "parent") {
            Some(Out::Any(Any::String(s))) => s.parse().ok(),
            _ => None,
        };
        if let Some(parent_id) = parent_id
            && let Some(parent_map) = self.node_map(&txn, parent_id)
            && let Some(Out::YArray(children)) = parent_map.get(&txn, "children")
        {
            let pos = children.iter(&txn).position(|out| match out {
                Out::Any(Any::String(s)) => s.parse::<BodyNodeId>().ok() == Some(id),
                _ => false,
            });
            if let Some(i) = pos {
                children.remove(&mut txn, i as u32);
            }
        }
        node.insert(&mut txn, "parent", yrs::In::Any(Any::Null));
        Ok(())
    }

    fn remove_subtree(&mut self, id: BodyNodeId) -> anyhow::Result<()> {
        self.detach_unchecked(id)?;
        let mut txn = self.doc.transact_mut();
        let mut subtree = vec![];
        collect_subtree(&self.nodes, &txn, id, &mut subtree);
        for desc in subtree {
            self.nodes.remove(&mut txn, &desc.to_string());
        }
        Ok(())
    }

    fn insert_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, value: &str) {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else { return };
        let Some(Out::YText(text)) = node.get(&txn, "text") else { return };
        let byte = byte_index_of(&text.get_string(&txn), char_index);
        text.insert(&mut txn, byte as u32, value);
    }

    fn remove_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, char_count: usize) {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else { return };
        let Some(Out::YText(text)) = node.get(&txn, "text") else { return };
        let current = text.get_string(&txn);
        let mut indices = current.char_indices().map(|(i, _)| i);
        let Some(start) = indices.nth(char_index) else { return };
        let end = indices.nth(char_count.saturating_sub(1)).unwrap_or(current.len());
        text.remove_range(&mut txn, start as u32, (end - start) as u32);
    }

    fn set_spec_unchecked(&mut self, id: BodyNodeId, spec: NodeSpec) -> anyhow::Result<()> {
        let mut txn = self.doc.transact_mut();
        let Some(node) = self.node_map(&txn, id) else {
            bail!("nœud inconnu : {id}")
        };
        let current_kind = read_kind(&node, &txn);
        let new_kind = spec.kind();
        if new_kind != current_kind {
            bail!(
                "impossible de remplacer {current_kind} par {new_kind} : genres différents"
            );
        }
        write_spec(&node, &mut txn, &spec);
        Ok(())
    }

    fn set_title(&mut self, title: &str) {
        let mut txn = self.doc.transact_mut();
        self.body.insert(&mut txn, "title", title);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn byte_index_of(text: &str, char_index: usize) -> usize {
    text.char_indices().nth(char_index).map(|(i, _)| i).unwrap_or(text.len())
}

fn collect_subtree(nodes: &yrs::MapRef, txn: &impl ReadTxn, id: BodyNodeId, out: &mut Vec<BodyNodeId>) {
    out.push(id);
    let Some(Out::YMap(node)) = nodes.get(txn, &id.to_string()) else { return };
    let Some(Out::YArray(children)) = node.get(txn, "children") else { return };
    let child_ids: Vec<BodyNodeId> = children
        .iter(txn)
        .filter_map(|out| match out {
            Out::Any(Any::String(s)) => s.parse().ok(),
            _ => None,
        })
        .collect();
    for cid in child_ids {
        collect_subtree(nodes, txn, cid, out);
    }
}

fn read_kind(node: &yrs::MapRef, txn: &impl ReadTxn) -> NodeKind {
    let Some(Out::Any(Any::String(s))) = node.get(txn, "kind") else {
        panic!("champ 'kind' invalide")
    };
    s.parse().unwrap_or_else(|_| panic!("kind invalide : {s}"))
}

fn read_bool(node: &yrs::MapRef, txn: &impl ReadTxn, key: &str) -> bool {
    matches!(node.get(txn, key), Some(Out::Any(Any::Bool(true))))
}

fn read_marker(node: &yrs::MapRef, txn: &impl ReadTxn, key: &str) -> ListMarker {
    match node.get(txn, key) {
        Some(Out::Any(Any::String(s))) => s.parse().unwrap_or_default(),
        _ => ListMarker::default(),
    }
}

fn read_u32(node: &yrs::MapRef, txn: &impl ReadTxn, key: &str) -> u32 {
    match node.get(txn, key) {
        Some(Out::Any(Any::Number(n))) => n as u32,
        _ => 0,
    }
}

fn read_spec(node: &yrs::MapRef, txn: &impl ReadTxn) -> NodeSpec {
    use NodeKind::*;
    match read_kind(node, txn) {
        Root => NodeSpec::Root,
        Visa => NodeSpec::Visa,
        Considerant => NodeSpec::Considerant,
        Sur => NodeSpec::Sur,
        Titre => NodeSpec::Titre(crate::kind::Titre { number: read_u32(node, txn, "number") }),
        LibelleTitre => NodeSpec::LibelleTitre,
        Section => NodeSpec::Section(crate::kind::Section { number: read_u32(node, txn, "number") }),
        LibelleSection => NodeSpec::LibelleSection,
        Chapitre => NodeSpec::Chapitre(crate::kind::Chapitre { number: read_u32(node, txn, "number") }),
        LibelleChapitre => NodeSpec::LibelleChapitre,
        Article => NodeSpec::Article(crate::kind::Article { number: read_u32(node, txn, "number") }),
        LibelleArticle => NodeSpec::LibelleArticle,
        ArticleBody => NodeSpec::ArticleBody,
        Annexe => NodeSpec::Annexe(crate::kind::Annexe { number: read_u32(node, txn, "number") }),
        LibelleAnnexe => NodeSpec::LibelleAnnexe,
        Paragraphe => NodeSpec::Paragraphe,
        Plain => NodeSpec::Plain(match node.get(txn, "text") {
            Some(Out::YText(t)) => t.get_string(txn),
            _ => String::new(),
        }),
        Span => NodeSpec::Span(content::Span {
            bold: read_bool(node, txn, "bold"),
            italic: read_bool(node, txn, "italic"),
            underline: read_bool(node, txn, "underline"),
            strikeout: read_bool(node, txn, "strikeout"),
        }),
        Table => NodeSpec::Table,
        TableRow => NodeSpec::TableRow,
        TableCell => NodeSpec::TableCell,
        List => NodeSpec::List(content::List {
            marker: read_marker(node, txn, "marker"),
            start: {
                let n = read_u32(node, txn, "start");
                if n == 0 { None } else { Some(n) }
            },
        }),
        ListItem => NodeSpec::ListItem(content::ListItem { marker: read_marker(node, txn, "marker") }),
    }
}

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
            node.insert(txn, "start", list.start.unwrap_or(0) as f64);
        }
        NodeSpec::ListItem(item) => {
            node.insert(txn, "marker", item.marker.as_ref());
        }
        NodeSpec::Titre(t) => { node.insert(txn, "number", t.number as f64); }
        NodeSpec::Section(s) => { node.insert(txn, "number", s.number as f64); }
        NodeSpec::Chapitre(c) => { node.insert(txn, "number", c.number as f64); }
        NodeSpec::Article(a) => { node.insert(txn, "number", a.number as f64); }
        NodeSpec::Annexe(a) => { node.insert(txn, "number", a.number as f64); }
        _ => {}
    }
}

fn node_prelim(spec: &NodeSpec, parent: Option<BodyNodeId>) -> MapPrelim {
    let mut fields: Vec<(&str, yrs::In)> = vec![
        ("kind", yrs::In::from(spec.kind().as_ref())),
        (
            "parent",
            parent
                .map(|p| yrs::In::from(p.to_string()))
                .unwrap_or(yrs::In::Any(Any::Null)),
        ),
        ("children", yrs::In::from(ArrayPrelim::default())),
    ];

    match spec {
        NodeSpec::Plain(text) => {
            fields.push(("text", yrs::In::from(TextPrelim::new(text.clone()))))
        }
        NodeSpec::Span(span) => {
            fields.push(("bold", yrs::In::from(span.bold)));
            fields.push(("italic", yrs::In::from(span.italic)));
            fields.push(("underline", yrs::In::from(span.underline)));
            fields.push(("strikeout", yrs::In::from(span.strikeout)));
        }
        NodeSpec::List(list) => {
            fields.push(("marker", yrs::In::from(list.marker.as_ref())));
            fields.push(("start", yrs::In::from(list.start.unwrap_or(0) as f64)));
        }
        NodeSpec::ListItem(item) => {
            fields.push(("marker", yrs::In::from(item.marker.as_ref())));
        }
        NodeSpec::Titre(t) => { fields.push(("number", yrs::In::from(t.number as f64))); }
        NodeSpec::Section(s) => { fields.push(("number", yrs::In::from(s.number as f64))); }
        NodeSpec::Chapitre(c) => { fields.push(("number", yrs::In::from(c.number as f64))); }
        NodeSpec::Article(a) => { fields.push(("number", yrs::In::from(a.number as f64))); }
        NodeSpec::Annexe(a) => { fields.push(("number", yrs::In::from(a.number as f64))); }
        _ => {}
    }

    MapPrelim::from_iter(fields)
}

#[cfg(test)]
mod tests {
    use crate::kind::{Titre, Article};
    use super::*;

    #[test]
    fn test_basic_yrs_body() {
        let mut body = YrsBody::new();
        let article = body
            .append_node(body.root(), NodeSpec::Article(Article::default()))
            .unwrap();
        assert_eq!(body.kind_of(article), NodeKind::Article);
        let leaf = body.first_leaf_of(article);
        assert_eq!(body.kind_of(leaf), NodeKind::Plain);
    }

    #[test]
    fn test_title_defaults_to_empty_and_is_settable() {
        let mut body = YrsBody::new();
        assert_eq!(body.title(), "");
        body.set_title("Arrêté préfectoral portant autorisation d'exploiter");
        assert_eq!(body.title(), "Arrêté préfectoral portant autorisation d'exploiter");
    }

    #[test]
    fn test_title_syncs_to_remote_doc() {
        use yrs::updates::decoder::Decode;

        let mut writer = YrsBody::new();
        writer.set_title("Titre de l'acte");

        let update = writer.doc().transact().encode_diff_v1(&yrs::StateVector::default());
        let remote_doc = Doc::new();
        remote_doc
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&update).unwrap())
            .unwrap();

        let remote_body_map = remote_doc.get_or_insert_map("body");
        let reader = YrsBody::open(remote_doc, remote_body_map).unwrap();
        assert_eq!(reader.title(), "Titre de l'acte");
    }

    #[test]
    fn test_open_from_synced_doc() {
        use yrs::updates::decoder::Decode;

        let mut writer = YrsBody::new();
        writer.append_node(writer.root(), NodeSpec::Titre(Titre::default())).unwrap();

        let update = writer.doc().transact().encode_diff_v1(&yrs::StateVector::default());
        let remote_doc = Doc::new();
        remote_doc
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&update).unwrap())
            .unwrap();

        let remote_body_map = remote_doc.get_or_insert_map("body");
        let reader = YrsBody::open(remote_doc, remote_body_map).unwrap();
        let children = reader.children_of(reader.root());
        assert_eq!(children.len(), 1);
        assert_eq!(reader.kind_of(children[0]), NodeKind::Titre);
    }
}
