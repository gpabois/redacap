use std::cmp::Ordering;

use anyhow::{anyhow, bail};

use crate::{ContentId, ContentKind, NodeSpec};

/// Accès en lecture à un arbre de [`Content`](crate) (contenu rich-text),
/// quel que soit le backend qui le stocke : structure en mémoire (mode
/// direct) ou document `yrs` (mode CRDT collaboratif).
///
/// Seules les méthodes "primitives" (`root`, `kind_of`, `text_of`,
/// `parent_of`, `children_of`) doivent être implémentées par un backend ;
/// le reste de la navigation (frères, ancêtres, feuilles, ordre des
/// feuilles...) est dérivé une seule fois ici, ce qui garantit que le mode
/// direct et le mode Yrs se comportent de manière identique.
pub trait ContentRead {
    fn root(&self) -> ContentId;
    fn kind_of(&self, id: ContentId) -> ContentKind;
    /// Texte porté par le noeud, vide si le noeud n'est pas terminal (`Plain`).
    fn text_of(&self, id: ContentId) -> String;
    fn parent_of(&self, id: ContentId) -> Option<ContentId>;
    /// Enfants directs, dans l'ordre du document.
    fn children_of(&self, id: ContentId) -> Vec<ContentId>;
    /// Spécification complète du noeud (attributs de mise en forme,
    /// marqueur de liste...), au-delà du simple discriminant `kind_of` ou du
    /// texte `text_of`.
    fn spec_of(&self, id: ContentId) -> NodeSpec;

    /// Longueur en nombre de caractères (et non en octets) du texte porté
    /// par le noeud.
    fn len_of(&self, id: ContentId) -> usize {
        self.text_of(id).chars().count()
    }

    fn first_child_of(&self, id: ContentId) -> Option<ContentId> {
        self.children_of(id).into_iter().next()
    }

    fn last_child_of(&self, id: ContentId) -> Option<ContentId> {
        self.children_of(id).into_iter().next_back()
    }

    fn prev_sibling_of(&self, id: ContentId) -> Option<ContentId> {
        let siblings = self.children_of(self.parent_of(id)?);
        let index = siblings.iter().position(|&sibling| sibling == id)?;
        index.checked_sub(1).map(|i| siblings[i])
    }

    fn next_sibling_of(&self, id: ContentId) -> Option<ContentId> {
        let siblings = self.children_of(self.parent_of(id)?);
        let index = siblings.iter().position(|&sibling| sibling == id)?;
        siblings.get(index + 1).copied()
    }

    /// Ancêtres de `id`, du parent direct jusqu'à la racine. `id` lui-même
    /// n'est pas inclus.
    fn ancestors_of(&self, id: ContentId) -> Vec<ContentId> {
        let mut ancestors = vec![];
        let mut current = id;
        while let Some(parent) = self.parent_of(current) {
            ancestors.push(parent);
            current = parent;
        }
        ancestors
    }

    fn first_leaf_of(&self, id: ContentId) -> ContentId {
        let mut current = id;
        while let Some(child) = self.first_child_of(current) {
            current = child;
        }
        current
    }

    fn last_leaf_of(&self, id: ContentId) -> ContentId {
        let mut current = id;
        while let Some(child) = self.last_child_of(current) {
            current = child;
        }
        current
    }

    /// Feuille précédente de `id` dans l'ordre du document (parcours infixe),
    /// `None` si `id` est la toute première feuille.
    fn prev_leaf_of(&self, id: ContentId) -> Option<ContentId> {
        std::iter::once(id)
            .chain(self.ancestors_of(id))
            .find_map(|node| self.prev_sibling_of(node))
            .map(|sibling| self.last_leaf_of(sibling))
    }

    /// Feuille suivante de `id` dans l'ordre du document (parcours infixe),
    /// `None` si `id` est la toute dernière feuille.
    fn next_leaf_of(&self, id: ContentId) -> Option<ContentId> {
        std::iter::once(id)
            .chain(self.ancestors_of(id))
            .find_map(|node| self.next_sibling_of(node))
            .map(|sibling| self.first_leaf_of(sibling))
    }

    /// Toutes les feuilles du document, dans l'ordre.
    fn leafs(&self) -> Vec<ContentId> {
        let mut leafs = vec![self.first_leaf_of(self.root())];
        while let Some(next) = self.next_leaf_of(*leafs.last().unwrap()) {
            leafs.push(next);
        }
        leafs
    }

    /// Compare la position de deux feuilles dans l'ordre du document.
    fn leaf_order_of(&self, lhs: ContentId, rhs: ContentId) -> Option<Ordering> {
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

/// Accès en écriture à un arbre de [`Content`](crate). Étend [`ContentRead`].
pub trait ContentWrite: ContentRead {
    /// Crée un noeud détaché (sans parent), à rattacher ensuite via
    /// [`ContentWrite::append_child`].
    fn create_node<N>(&mut self, spec: N) -> ContentId
    where
        NodeSpec: From<N>,
        Self: Sized;

    /// Insère `child` comme enfant de `parent` à la position `index` (`0` =
    /// premier enfant, `children_of(parent).len()` = dernier), sans
    /// vérifier la compatibilité des types de noeuds. `child` doit être
    /// détaché (sans parent) au préalable.
    fn insert_child_at(
        &mut self,
        parent: ContentId,
        index: usize,
        child: ContentId,
    ) -> anyhow::Result<()>;

    /// Détache `id` de son parent, sans le supprimer : il devient un noeud
    /// sans parent, prêt à être rattaché ailleurs via
    /// [`ContentWrite::insert_child_at`]. Ne fait rien si `id` n'a déjà pas
    /// de parent.
    fn detach_unchecked(&mut self, id: ContentId) -> anyhow::Result<()>;

    /// Détache `id` (s'il a un parent) puis le supprime définitivement,
    /// ainsi que tous ses descendants.
    fn remove_node(&mut self, id: ContentId) -> anyhow::Result<()>;

    fn insert_text(&mut self, id: ContentId, char_index: usize, value: &str);

    /// Retire `char_count` caractères à partir de `char_index`.
    fn remove_text(&mut self, id: ContentId, char_index: usize, char_count: usize);

    /// Remplace les attributs du noeud par ceux de `spec`, sans toucher à sa
    /// position dans l'arbre (parent, enfants). `spec` doit être du même
    /// [`ContentKind`] que le noeud existant, sous peine d'erreur.
    fn set_spec(&mut self, id: ContentId, spec: NodeSpec) -> anyhow::Result<()>;

    /// Rattache `child` comme dernier enfant de `parent`, sans vérifier la
    /// compatibilité des types de noeuds.
    fn append_child_unchecked(&mut self, parent: ContentId, child: ContentId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let index = self.children_of(parent).len();
        self.insert_child_at(parent, index, child)
    }

    /// Insère `sibling` (détaché au préalable) juste après `id`, comme
    /// enfant du même parent que `id`.
    fn insert_sibling_after(
        &mut self,
        id: ContentId,
        sibling: ContentId,
    ) -> anyhow::Result<ContentId>
    where
        Self: Sized,
    {
        let parent = self
            .parent_of(id)
            .ok_or_else(|| anyhow!("le noeud {id} n'a pas de parent"))?;
        let siblings = self.children_of(parent);
        let index = siblings
            .iter()
            .position(|&s| s == id)
            .ok_or_else(|| anyhow!("le noeud {id} n'est pas un enfant de son propre parent"))?;

        self.insert_child_at(parent, index + 1, sibling)?;
        Ok(sibling)
    }

    /// Fusionne `source` dans `target` : pour deux noeuds `Plain`,
    /// concatène leurs textes (celui de `source` après celui de `target`) ;
    /// pour deux noeuds non-terminaux acceptant les mêmes genres d'enfants
    /// (ex: deux `Paragraph`, ou un `Paragraph` et un `ListItem`, tous deux
    /// faits de `Plain`/`Span`), déplace les enfants de `source` à la fin
    /// de ceux de `target`, quel que soit le genre exact de chacun —
    /// `target` conserve son propre genre et ses propres attributs.
    /// `source` est ensuite supprimé.
    fn merge_into(&mut self, target: ContentId, source: ContentId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        let target_kind = self.kind_of(target);
        let source_kind = self.kind_of(source);

        match (target_kind, source_kind) {
            (ContentKind::Plain, ContentKind::Plain) => {
                let merged = format!("{}{}", self.text_of(target), self.text_of(source));
                self.set_spec(target, NodeSpec::Plain(merged))?;
            }
            (t, s)
                if t != ContentKind::Plain
                    && s != ContentKind::Plain
                    && t.allowed_children_match(s) =>
            {
                for child in self.children_of(source) {
                    self.detach_unchecked(child)?;
                    self.append_child_unchecked(target, child)?;
                }
            }
            (t, s) => bail!(
                "impossible de fusionner un noeud {s} dans un noeud {t} : structures incompatibles"
            ),
        }

        self.remove_node(source)
    }

    /// Fusionne `id` avec son frère précédent (même parent), voir
    /// [`ContentWrite::merge_into`] pour les genres acceptés. Renvoie
    /// l'identifiant du noeud fusionné (le frère précédent), `id` ayant été
    /// supprimé.
    fn merge_with_prev(&mut self, id: ContentId) -> anyhow::Result<ContentId>
    where
        Self: Sized,
    {
        let prev = self
            .prev_sibling_of(id)
            .ok_or_else(|| anyhow!("le noeud {id} n'a pas de frère précédent"))?;
        self.merge_into(prev, id)?;
        Ok(prev)
    }

    /// Divise `id` en deux noeuds de même nature au point `at` : pour un
    /// noeud `Plain`, `at` est un index de caractère ; pour tout autre
    /// noeud, un index d'enfant. La partie à partir de `at` est déplacée
    /// dans un nouveau noeud frère, inséré juste après `id`. Renvoie
    /// l'identifiant de ce nouveau noeud.
    fn split_node(&mut self, id: ContentId, at: usize) -> anyhow::Result<ContentId>
    where
        Self: Sized,
    {
        let spec = self.spec_of(id);

        let new_id = if let NodeSpec::Plain(text) = spec {
            let byte_at = text
                .char_indices()
                .nth(at)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            let head = text[..byte_at].to_string();
            let tail = text[byte_at..].to_string();

            self.set_spec(id, NodeSpec::Plain(head))?;
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

    /// Insère les noeuds intermédiaires manquants entre `descendant` et
    /// `target` pour que `descendant` devienne un descendant valide de
    /// `target`, et renvoie l'identifiant du noeud directement attachable à
    /// `target`.
    fn ensure_compatible_node_for(
        &mut self,
        target: ContentId,
        descendant: ContentId,
    ) -> anyhow::Result<ContentId>
    where
        Self: Sized,
    {
        let parent_kind = self.kind_of(target);
        let kind = self.kind_of(descendant);

        let mut path = kind.correction_path(parent_kind).ok_or_else(|| {
            anyhow!("noeud de contenu enfant incompatible avec le parent {kind} vs. {parent_kind}")
        })?;
        path.reverse();

        let mut content_id = descendant;
        for kind in path {
            let parent_id = self.create_node(kind.new_default_node());
            self.append_child_unchecked(parent_id, content_id)?;
            content_id = parent_id;
        }

        Ok(content_id)
    }

    /// Garantit que toute feuille du document est un noeud `Plain`, en
    /// insérant un `Plain` vide sous chaque feuille qui ne le serait pas.
    fn ensure_only_plain_leafs(&mut self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        for leaf in self.leafs() {
            if self.kind_of(leaf) != ContentKind::Plain {
                let plain_id = self.create_node("");
                let compat_id = self.ensure_compatible_node_for(leaf, plain_id)?;
                self.append_child_unchecked(leaf, compat_id)?;
            }
        }
        Ok(())
    }

    /// Rattache `child` comme dernier enfant de `parent` et restaure
    /// l'invariant "seules les feuilles sont des `Plain`".
    fn append_child(&mut self, parent: ContentId, child: ContentId) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        self.append_child_unchecked(parent, child)?;
        self.ensure_only_plain_leafs()
    }

    /// Crée un noeud à partir de `spec`, l'insère sous un ancêtre compatible
    /// de `parent` (en créant les noeuds intermédiaires nécessaires) et
    /// renvoie son identifiant.
    fn append_content<N>(&mut self, parent: ContentId, spec: N) -> anyhow::Result<ContentId>
    where
        NodeSpec: From<N>,
        Self: Sized,
    {
        let content_id = self.create_node(spec);
        let compat_id = self.ensure_compatible_node_for(parent, content_id)?;
        self.append_child(parent, compat_id)?;
        Ok(content_id)
    }
}
