use std::cmp::Ordering;

use anyhow::{anyhow, bail};

use crate::{BodyNodeId, NodeKind, NodeSpec};

/// Accès en lecture au corps d'un acte légal, quel que soit le backend
/// (mémoire directe ou `yrs`). Seules les cinq méthodes primitives
/// (`root`, `kind_of`, `text_of`, `parent_of`, `children_of`) doivent
/// être implémentées ; le reste de la navigation est dérivé ici.
pub trait BodyRead {
    fn root(&self) -> BodyNodeId;
    fn kind_of(&self, id: BodyNodeId) -> NodeKind;
    /// Texte du nœud — non vide uniquement pour les nœuds `Plain`.
    fn text_of(&self, id: BodyNodeId) -> String;
    fn parent_of(&self, id: BodyNodeId) -> Option<BodyNodeId>;
    /// Enfants directs dans l'ordre du document.
    fn children_of(&self, id: BodyNodeId) -> Vec<BodyNodeId>;
    /// Spécification complète du nœud (attributs inclus).
    fn spec_of(&self, id: BodyNodeId) -> NodeSpec;
    /// Titre de l'acte (ex. « Arrêté préfectoral portant autorisation
    /// d'exploiter... »), vide tant qu'il n'a pas été renseigné. Propriété du
    /// document dans son ensemble, distincte des nœuds `Titre` du corps
    /// (subdivisions numérotées « Titre I », « Titre II »...).
    fn title(&self) -> String;

    fn len_of(&self, id: BodyNodeId) -> usize {
        self.text_of(id).chars().count()
    }

    fn first_child_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        self.children_of(id).into_iter().next()
    }

    fn last_child_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        self.children_of(id).into_iter().next_back()
    }

    fn prev_sibling_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        let siblings = self.children_of(self.parent_of(id)?);
        let index = siblings.iter().position(|&s| s == id)?;
        index.checked_sub(1).map(|i| siblings[i])
    }

    fn next_sibling_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        let siblings = self.children_of(self.parent_of(id)?);
        let index = siblings.iter().position(|&s| s == id)?;
        siblings.get(index + 1).copied()
    }

    fn ancestors_of(&self, id: BodyNodeId) -> Vec<BodyNodeId> {
        let mut ancestors = vec![];
        let mut current = id;
        while let Some(parent) = self.parent_of(current) {
            ancestors.push(parent);
            current = parent;
        }
        ancestors
    }

    fn first_leaf_of(&self, id: BodyNodeId) -> BodyNodeId {
        let mut current = id;
        while let Some(child) = self.first_child_of(current) {
            current = child;
        }
        current
    }

    fn last_leaf_of(&self, id: BodyNodeId) -> BodyNodeId {
        let mut current = id;
        while let Some(child) = self.last_child_of(current) {
            current = child;
        }
        current
    }

    /// Feuille `Plain` précédente dans l'ordre du document.
    fn prev_leaf_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        std::iter::once(id)
            .chain(self.ancestors_of(id))
            .find_map(|node| self.prev_sibling_of(node))
            .map(|sibling| self.last_leaf_of(sibling))
    }

    /// Feuille `Plain` suivante dans l'ordre du document.
    fn next_leaf_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        std::iter::once(id)
            .chain(self.ancestors_of(id))
            .find_map(|node| self.next_sibling_of(node))
            .map(|sibling| self.first_leaf_of(sibling))
    }

    /// Toutes les feuilles `Plain` du corps, dans l'ordre du document.
    fn leafs(&self) -> Vec<BodyNodeId> {
        let mut leafs = vec![self.first_leaf_of(self.root())];
        while let Some(next) = self.next_leaf_of(*leafs.last().unwrap()) {
            leafs.push(next);
        }
        leafs
    }

    /// Ordre relatif de deux feuilles dans le document.
    fn leaf_order_of(&self, lhs: BodyNodeId, rhs: BodyNodeId) -> Option<Ordering> {
        if lhs == rhs {
            return Some(Ordering::Equal);
        }
        for leaf in self.leafs() {
            if leaf == lhs {
                return Some(Ordering::Less);
            }
            if leaf == rhs {
                return Some(Ordering::Greater);
            }
        }
        None
    }
}

/// Accès en écriture au corps d'un acte légal. Étend [`BodyRead`].
///
/// Les méthodes `_unchecked` modifient l'arbre sans vérifier les
/// invariants structurels ; les méthodes sans suffixe les maintiennent.
pub trait BodyWrite: BodyRead {
    // ── Primitives (non vérifiées) ──────────────────────────────────────

    fn create_node(&mut self, spec: NodeSpec) -> BodyNodeId;

    fn insert_child_at_unchecked(
        &mut self,
        parent: BodyNodeId,
        index: usize,
        child: BodyNodeId,
    ) -> anyhow::Result<()>;

    fn detach_unchecked(&mut self, id: BodyNodeId) -> anyhow::Result<()>;

    fn remove_subtree(&mut self, id: BodyNodeId) -> anyhow::Result<()>;

    fn insert_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, value: &str);

    fn remove_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, char_count: usize);

    fn set_spec_unchecked(&mut self, id: BodyNodeId, spec: NodeSpec) -> anyhow::Result<()>;

    /// Définit le titre de l'acte (voir [`BodyRead::title`]).
    fn set_title(&mut self, title: &str);

    // ── Navigation dérivée ──────────────────────────────────────────────

    fn append_child_unchecked(
        &mut self,
        parent: BodyNodeId,
        child: BodyNodeId,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let index = self.children_of(parent).len();
        self.insert_child_at_unchecked(parent, index, child)
    }

    fn insert_sibling_after(
        &mut self,
        id: BodyNodeId,
        sibling: BodyNodeId,
    ) -> anyhow::Result<BodyNodeId>
    where
        Self: Sized,
    {
        let parent = self
            .parent_of(id)
            .ok_or_else(|| anyhow!("le nœud {id} n'a pas de parent"))?;
        let index = self
            .children_of(parent)
            .iter()
            .position(|&s| s == id)
            .ok_or_else(|| anyhow!("le nœud {id} est orphelin de son parent"))?;
        self.insert_child_at_unchecked(parent, index + 1, sibling)?;
        Ok(sibling)
    }

    // ── Invariants structurels ──────────────────────────────────────────

    /// Insère `child` sous `parent` à la position correcte pour respecter
    /// les règles d'ordre (pour les enfants du Root : Visa < Considerant <
    /// Sur < structurel < Annexe) et les règles d'appartenance.
    fn append_node(
        &mut self,
        parent: BodyNodeId,
        spec: NodeSpec,
    ) -> anyhow::Result<BodyNodeId>
    where
        Self: Sized,
    {
        let parent_kind = self.kind_of(parent);
        let child_kind = spec.kind();

        if !parent_kind.can_accept_child(child_kind) {
            // Redirige vers le conteneur de contenu (ex. Article → ArticleBody)
            // si `parent` en a un et qu'il accepte `child_kind`, pour que les
            // appelants puissent continuer à cibler le nœud structurel
            // directement sans connaître l'existence du conteneur.
            if let Some(container_kind) = parent_kind.content_container_kind()
                && container_kind.can_accept_child(child_kind)
            {
                let container = self
                    .children_of(parent)
                    .into_iter()
                    .find(|&c| self.kind_of(c) == container_kind)
                    .ok_or_else(|| anyhow!("{container_kind} manquant sous {parent_kind}"))?;
                return self.append_node(container, spec);
            }
            bail!(
                "le nœud {child_kind} n'est pas autorisé comme enfant de {parent_kind}"
            );
        }

        let id = self.create_node(spec);
        let index = self.insertion_index_in(parent, child_kind);
        self.insert_child_at_unchecked(parent, index, id)?;
        self.ensure_only_plain_leafs()?;

        if child_kind.is_numbered() {
            self.renumber_siblings(parent, child_kind)?;
        }

        Ok(id)
    }

    /// Calcule l'index d'insertion pour `child_kind` sous `parent`,
    /// en respectant l'ordre des groupes dans Root.
    fn insertion_index_in(&self, parent: BodyNodeId, child_kind: NodeKind) -> usize
    where
        Self: Sized,
    {
        if self.kind_of(parent) != NodeKind::Root {
            return self.children_of(parent).len();
        }

        let child_group = child_kind.root_order_group().unwrap_or(u8::MAX);
        self.children_of(parent)
            .iter()
            .take_while(|&&sibling| {
                self.kind_of(sibling)
                    .root_order_group()
                    .map(|g| g <= child_group)
                    .unwrap_or(false)
            })
            .count()
    }

    /// Garantit que toute feuille est un nœud `Plain`, en créant le
    /// sous-arbre minimal obligatoire sous chaque feuille non-terminale.
    fn ensure_only_plain_leafs(&mut self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let non_plain_leafs: Vec<BodyNodeId> = self
            .leafs_raw()
            .into_iter()
            .filter(|&id| self.kind_of(id) != NodeKind::Plain)
            .collect();

        for id in non_plain_leafs {
            self.ensure_leaf_has_plain(id)?;
        }
        Ok(())
    }

    /// Crée le sous-arbre minimum pour que `id` ait au moins un descendant
    /// `Plain`. Le comportement dépend du type du nœud :
    ///
    /// - Nœud avec un `label_child_kind` (Titre, Article, Annexe…) :
    ///   crée `Libellé → Plain`. Pour `Article`, crée aussi
    ///   `ArticleBody → Paragraphe → Plain`.
    /// - Nœud acceptant directement `Plain` (Visa, Paragraphe, ListItem…) :
    ///   crée `Plain`.
    /// - Nœuds de contenu sans `Plain` direct (Table, List…) :
    ///   crée la chaîne minimale jusqu'à `Plain`.
    fn ensure_leaf_has_plain(&mut self, id: BodyNodeId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let kind = self.kind_of(id);

        if kind == NodeKind::Plain {
            return Ok(());
        }

        if kind.can_accept_child(NodeKind::Plain) {
            let plain = self.create_node(NodeSpec::Plain(String::new()));
            return self.append_child_unchecked(id, plain);
        }

        if let Some(label_kind) = kind.label_child_kind() {
            // Crée Libellé → Plain
            let label = self.create_node(label_kind.default_spec());
            self.append_child_unchecked(id, label)?;
            let plain = self.create_node(NodeSpec::Plain(String::new()));
            self.append_child_unchecked(label, plain)?;

            // Pour Article : crée aussi ArticleBody → Paragraphe → Plain
            // (corps éditable, distinct du libellé).
            if kind == NodeKind::Article {
                let article_body = self.create_node(NodeSpec::ArticleBody);
                self.append_child_unchecked(id, article_body)?;
                let para = self.create_node(NodeSpec::Paragraphe);
                self.append_child_unchecked(article_body, para)?;
                let plain2 = self.create_node(NodeSpec::Plain(String::new()));
                self.append_child_unchecked(para, plain2)?;
            }
            return Ok(());
        }

        // Nœuds de contenu sans Plain direct : créer la chaîne minimale.
        match kind {
            NodeKind::Table => {
                let row = self.create_node(NodeSpec::TableRow);
                self.append_child_unchecked(id, row)?;
                let cell = self.create_node(NodeSpec::TableCell);
                self.append_child_unchecked(row, cell)?;
                let para = self.create_node(NodeSpec::Paragraphe);
                self.append_child_unchecked(cell, para)?;
                let plain = self.create_node(NodeSpec::Plain(String::new()));
                self.append_child_unchecked(para, plain)?;
            }
            NodeKind::TableRow => {
                let cell = self.create_node(NodeSpec::TableCell);
                self.append_child_unchecked(id, cell)?;
                let para = self.create_node(NodeSpec::Paragraphe);
                self.append_child_unchecked(cell, para)?;
                let plain = self.create_node(NodeSpec::Plain(String::new()));
                self.append_child_unchecked(para, plain)?;
            }
            NodeKind::TableCell => {
                let para = self.create_node(NodeSpec::Paragraphe);
                self.append_child_unchecked(id, para)?;
                let plain = self.create_node(NodeSpec::Plain(String::new()));
                self.append_child_unchecked(para, plain)?;
            }
            NodeKind::List => {
                let item = self.create_node(NodeSpec::ListItem(content::ListItem::default()));
                self.append_child_unchecked(id, item)?;
                let plain = self.create_node(NodeSpec::Plain(String::new()));
                self.append_child_unchecked(item, plain)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Toutes les feuilles (nœuds sans enfant), y compris les non-Plain.
    fn leafs_raw(&self) -> Vec<BodyNodeId>
    where
        Self: Sized,
    {
        fn collect(body: &dyn BodyRead, id: BodyNodeId, out: &mut Vec<BodyNodeId>) {
            let children = body.children_of(id);
            if children.is_empty() {
                out.push(id);
            } else {
                for child in children {
                    collect(body, child, out);
                }
            }
        }
        let mut out = vec![];
        collect(self, self.root(), &mut out);
        out
    }

    // ── Texte ───────────────────────────────────────────────────────────

    /// Insère du texte dans un nœud `Plain` et déplace le curseur.
    fn insert_text(&mut self, id: BodyNodeId, char_index: usize, value: &str) {
        self.insert_text_unchecked(id, char_index, value);
    }

    /// Supprime du texte dans un nœud `Plain`. Si le nœud devient vide,
    /// déclenche la suppression en cascade des ancêtres de contenu vides,
    /// avec recréation des nœuds obligatoires (voir exigences).
    fn remove_text(
        &mut self,
        id: BodyNodeId,
        char_index: usize,
        char_count: usize,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        self.remove_text_unchecked(id, char_index, char_count);
        if self.text_of(id).is_empty() {
            self.cascade_delete_plain(id)?;
        }
        Ok(())
    }

    /// Supprime un nœud `Plain` vide et propage la suppression vers les
    /// parents de contenu vides, jusqu'au premier ancêtre non-contenu.
    fn cascade_delete_plain(&mut self, id: BodyNodeId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let parent = match self.parent_of(id) {
            Some(p) => p,
            None => return Ok(()),
        };
        self.remove_subtree(id)?;

        let mut current = parent;
        loop {
            if !self.children_of(current).is_empty() {
                break;
            }
            let parent_kind = self.kind_of(current);
            if !parent_kind.is_content_node() {
                // Nœud non-contenu vide : recréer le sous-arbre obligatoire.
                self.recreate_mandatory_leaf(current)?;
                break;
            }
            let next_parent = match self.parent_of(current) {
                Some(p) => p,
                None => break,
            };
            self.remove_subtree(current)?;
            current = next_parent;
        }
        Ok(())
    }

    /// Recrée le plain minimum obligatoire sous un nœud non-contenu vide.
    fn recreate_mandatory_leaf(&mut self, id: BodyNodeId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let kind = self.kind_of(id);
        if kind.is_label() || matches!(kind, NodeKind::Visa | NodeKind::Considerant | NodeKind::Sur) {
            // Libellé* ou visa/considérant/sur → recréer directement un Plain
            let plain = self.create_node(NodeSpec::Plain(String::new()));
            self.append_child_unchecked(id, plain)?;
        } else if kind == NodeKind::ArticleBody {
            // ArticleBody → recréer un Paragraphe > Plain
            let para = self.create_node(NodeSpec::Paragraphe);
            self.append_child_unchecked(id, para)?;
            let plain = self.create_node(NodeSpec::Plain(String::new()));
            self.append_child_unchecked(para, plain)?;
        }
        Ok(())
    }

    // ── Fusion / Division (nœuds de contenu uniquement) ─────────────────

    /// Fusionne `source` dans `target` (les deux doivent être des nœuds
    /// de contenu de structures compatibles). Voir les exigences pour les
    /// règles entre Paragraphe, List et ListItem.
    fn merge_into(
        &mut self,
        target: BodyNodeId,
        source: BodyNodeId,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let tk = self.kind_of(target);
        let sk = self.kind_of(source);

        if !tk.is_content_node() || !sk.is_content_node() {
            bail!("la fusion n'est possible que pour les nœuds de contenu ({tk} / {sk})");
        }

        match (tk, sk) {
            (NodeKind::Plain, NodeKind::Plain) => {
                let merged = format!("{}{}", self.text_of(target), self.text_of(source));
                self.set_spec_unchecked(target, NodeSpec::Plain(merged))?;
            }

            // Paragraphe ← List : prendre les enfants du premier ListItem
            (NodeKind::Paragraphe, NodeKind::List) => {
                if let Some(first_item) = self.first_child_of(source) {
                    for child in self.children_of(first_item) {
                        self.detach_unchecked(child)?;
                        self.append_child_unchecked(target, child)?;
                    }
                }
            }

            // List ← Paragraphe : ajouter les enfants dans le dernier ListItem
            (NodeKind::List, NodeKind::Paragraphe) => {
                if let Some(last_item) = self.last_child_of(target) {
                    for child in self.children_of(source) {
                        self.detach_unchecked(child)?;
                        self.append_child_unchecked(last_item, child)?;
                    }
                } else {
                    // List vide : on supprime simplement source
                }
            }

            // Cas général : même type ou types à enfants compatibles
            _ => {
                for child in self.children_of(source) {
                    self.detach_unchecked(child)?;
                    self.append_child_unchecked(target, child)?;
                }
            }
        }

        self.remove_subtree(source)
    }

    /// Fusionne `id` avec son frère précédent.
    fn merge_with_prev(&mut self, id: BodyNodeId) -> anyhow::Result<BodyNodeId>
    where
        Self: Sized,
    {
        let prev = self
            .prev_sibling_of(id)
            .ok_or_else(|| anyhow!("le nœud {id} n'a pas de frère précédent"))?;
        self.merge_into(prev, id)?;
        Ok(prev)
    }

    /// Divise le nœud `id` (nœud de contenu uniquement) au point `at`
    /// (index de caractère pour `Plain`, index d'enfant pour les autres).
    /// Renvoie l'identifiant du nouveau nœud inséré juste après `id`.
    fn split_node(&mut self, id: BodyNodeId, at: usize) -> anyhow::Result<BodyNodeId>
    where
        Self: Sized,
    {
        let kind = self.kind_of(id);
        if !kind.is_content_node() {
            bail!("la division n'est possible que pour les nœuds de contenu ({kind})");
        }

        let spec = self.spec_of(id);

        let new_id = if let NodeSpec::Plain(text) = spec {
            let byte_at = text
                .char_indices()
                .nth(at)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            let head = text[..byte_at].to_string();
            let tail = text[byte_at..].to_string();
            self.set_spec_unchecked(id, NodeSpec::Plain(head))?;
            self.create_node(NodeSpec::Plain(tail))
        } else {
            let new_id = self.create_node(spec);
            for child in self.children_of(id).into_iter().skip(at) {
                self.detach_unchecked(child)?;
                self.append_child_unchecked(new_id, child)?;
            }
            new_id
        };

        self.insert_sibling_after(id, new_id)
    }

    // ── Numérotation ────────────────────────────────────────────────────

    /// Recalcule la numérotation de tous les nœuds `kind` dans les enfants
    /// directs de `parent` (numérotation locale, 1-based).
    fn renumber_siblings(
        &mut self,
        parent: BodyNodeId,
        kind: NodeKind,
    ) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let mut counter = 1u32;
        for sibling in self.children_of(parent) {
            let sk = self.kind_of(sibling);
            if sk == kind {
                let spec = self.spec_of(sibling).with_number(counter);
                self.set_spec_unchecked(sibling, spec)?;
                counter += 1;
            }
        }
        Ok(())
    }

    /// Recalcule la numérotation de toutes les Annexes dans le Root.
    fn renumber_annexes(&mut self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        self.renumber_siblings(self.root(), NodeKind::Annexe)
    }

    /// Supprime un nœud structurel ou de contenu avec mise à jour de
    /// la numérotation. Le Root ne peut pas être supprimé.
    fn remove_node(&mut self, id: BodyNodeId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        if id == self.root() {
            bail!("le nœud Root ne peut pas être supprimé");
        }
        let kind = self.kind_of(id);
        let parent = self.parent_of(id);
        self.remove_subtree(id)?;
        if let (Some(parent), true) = (parent, kind.is_numbered()) {
            self.renumber_siblings(parent, kind)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_root_order<B: BodyRead>(body: &B) {
        let root = body.root();
        let mut last_group = 0u8;
        for child in body.children_of(root) {
            let group = body
                .kind_of(child)
                .root_order_group()
                .unwrap_or(u8::MAX);
            assert!(
                group >= last_group,
                "violation d'ordre dans Root : groupe {group} après groupe {last_group}"
            );
            last_group = group;
        }
    }

    // Les tests concrets sont dans direct.rs avec une implémentation réelle.
    // Ici on vérifie la logique pure des helpers.

    #[test]
    fn test_root_order_groups_are_monotone() {
        use NodeKind::*;
        // Simule l'ordre attendu
        let order = [Visa, Considerant, Sur, Titre, Article, Annexe];
        let groups: Vec<u8> = order.iter().map(|k| k.root_order_group().unwrap()).collect();
        for w in groups.windows(2) {
            assert!(w[0] <= w[1]);
        }
    }
}
