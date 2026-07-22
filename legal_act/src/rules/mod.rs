use crate::{data::NodeKind, model::LegalActProject, id::NodeId};

pub use NodeKind::*;

pub const ALLOWED_BODY_STRUCTURE: &'static [(NodeKind, &'static [NodeKind])] = &[
    (
        VisaRoot,
        &[Visa]
    ),
    (
        Visa,
        &[Span, Plain]
    ),
    (
        ConsiderantRoot,
        &[Considerant]
    ),
    (
        Considerant,
        &[Span, Plain]
    ),
    (
        SurRoot,
        &[Sur]
    ),
    (
        Sur,
        &[Span, Plain]
    ),
    (
        BodyRoot,
        &[Titre, Section, Chapitre, Article, Annexe]
    ),
    (
        Titre,
        &[Section, Chapitre, Article]
    ),
    (
        Section,
        &[Chapitre, Article]
    ),
    (
        Chapitre,
        &[Article]
    ),
    (
        Annexe,
        &[Titre, Section, Chapitre, Article]
    ),
    (
        Article,
        &[LibelleArticle, ArticleBody]
    ),
    (
        LibelleArticle,
        &[Plain]
    ),
    (
        ArticleBody,
        &[Paragraphe, List, Table]
    ),
    (
        Paragraphe,
        &[Plain, Span]
    ),
    (
        Span,
        &[Plain]
    ),
    (
        Table,
        &[TableRow]
    ),
    (
        TableRow,
        &[TableCell]
    ),
    (
        TableCell,
        &[Paragraphe, List]
    ),
    (
        List,
        &[ListItem]
    ),
    (
        ListItem,
        &[List, Span, Plain]
    )
];

/// Enfants directs autorisés pour `kind` d'après [`ALLOWED_BODY_STRUCTURE`].
fn allowed_children(kind: NodeKind) -> &'static [NodeKind] {
    ALLOWED_BODY_STRUCTURE
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, children)| *children)
        .unwrap_or(&[])
}

/// Calcule le chemin le plus court de nœuds intermédiaires à créer entre
/// `from` et `to` pour respecter [`ALLOWED_BODY_STRUCTURE`].
///
/// Le résultat exclut `from` et `to` : il ne contient que les nœuds à
/// insérer entre les deux. Retourne `Some(vec![])` si `to` est déjà un
/// enfant direct autorisé de `from`, et `None` si aucun chemin n'existe.
///
/// `ALLOWED_BODY_STRUCTURE` est un graphe orienté qui peut boucler (p. ex.
/// `List` ⇄ `ListItem`), la recherche est donc une BFS avec un ensemble de
/// nœuds déjà visités pour garantir la terminaison et l'optimalité du
/// chemin trouvé.
pub fn intermediary_nodes(from: NodeKind, to: NodeKind) -> Option<Vec<NodeKind>> {
    if allowed_children(from).contains(&to) {
        return Some(Vec::new());
    }

    let mut visited = vec![from];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(vec![from]);

    while let Some(path) = queue.pop_front() {
        let current = *path.last().unwrap();
        for &child in allowed_children(current) {
            if child == to {
                return Some(path[1..].to_vec());
            }
            if !visited.contains(&child) {
                visited.push(child);
                let mut next_path = path.clone();
                next_path.push(child);
                queue.push_back(next_path);
            }
        }
    }

    None
}

/// S'assure que la structure de l'acte légal est conforme
/// 
/// Dans le cas d'un corps d'article, s'assure que toutes les feuilles
/// sont bien des noeuds textuels. 
pub fn assert_structure(act: &LegalActProject, from: &NodeId) {
    use NodeKind::*;

    let from_kind = act.kind(from);

    for mut leaf in act.leafs(from).filter(|leaf| act.kind(&leaf) != Plain) {
        let Some(intermediary) = intermediary_nodes(from_kind, act.kind(&leaf)) else { continue };
        if intermediary.len() == 0 { continue }

        for kind in intermediary {
            let id = act.create_node(kind);
            act.append_child(&leaf, &id);
            leaf = id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_child_needs_no_intermediary() {
        assert_eq!(intermediary_nodes(Paragraphe, Plain), Some(vec![]));
    }

    #[test]
    fn list_to_span_goes_through_list_item() {
        assert_eq!(intermediary_nodes(List, Span), Some(vec![ListItem]));
    }

    #[test]
    fn body_root_to_plain_shortest_path() {
        assert_eq!(intermediary_nodes(BodyRoot, Plain), Some(vec![Article, LibelleArticle]));
    }

    #[test]
    fn no_path_returns_none() {
        assert_eq!(intermediary_nodes(Plain, BodyRoot), None);
    }
}