use crate::{ContentHandle, ContentId, ContentRead, ContentWrite};

/// Fusionne `source` dans `target` (texte ou enfants selon le genre), puis
/// fusionne récursivement leurs ancêtres devenus orphelins de contenu, tant
/// qu'ils diffèrent : ceci permet de fusionner deux feuilles situées dans
/// des blocs différents (ex: deux paragraphes) sans perdre les noeuds
/// restants de part et d'autre.
pub(super) fn merge_leaves(body: &mut ContentHandle, target: ContentId, source: ContentId) {
    let mut target_parent = body.parent_of(target);
    let mut source_parent = body.parent_of(source);

    if body.merge_into(target, source).is_err() {
        return;
    }

    while let (Some(t), Some(s)) = (target_parent, source_parent) {
        if t == s {
            break;
        }

        target_parent = body.parent_of(t);
        source_parent = body.parent_of(s);

        if body.merge_into(t, s).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ContentHandle;

    use super::*;

    #[test]
    fn test_merge_leaves_across_paragraphs() {
        let mut body = ContentHandle::direct();
        let first = body.append_content(body.root(), "hello").unwrap();
        let second = body.append_content(body.root(), "world").unwrap();

        merge_leaves(&mut body, first, second);

        assert_eq!(body.text_of(first), "helloworld");
        assert_eq!(body.children_of(body.root()), vec![body.parent_of(first).unwrap()]);
    }

    #[test]
    fn test_merge_leaves_same_paragraph_keeps_single_block() {
        let mut body = ContentHandle::direct();
        let leaf = body.append_content(body.root(), "a").unwrap();
        let paragraph = body.parent_of(leaf).unwrap();

        let first = body.create_node("a");
        let second = body.create_node("b");
        body.insert_child_at(paragraph, 1, first).unwrap();
        body.insert_child_at(paragraph, 2, second).unwrap();

        merge_leaves(&mut body, first, second);

        assert_eq!(body.text_of(first), "ab");
        assert_eq!(body.children_of(body.root()).len(), 1);
        assert_eq!(body.children_of(paragraph), vec![leaf, first]);
    }

    #[test]
    fn test_merge_leaves_paragraph_into_preceding_list() {
        use crate::{List, ListItem};

        let mut body = ContentHandle::direct();

        let list = body.create_node(List::default());
        body.insert_child_at(body.root(), 0, list).unwrap();

        let item1 = body.create_node(ListItem::default());
        body.insert_child_at(list, 0, item1).unwrap();
        let a = body.create_node("a");
        body.insert_child_at(item1, 0, a).unwrap();

        let item2 = body.create_node(ListItem::default());
        body.insert_child_at(list, 1, item2).unwrap();
        let b = body.create_node("b");
        body.insert_child_at(item2, 0, b).unwrap();

        let paragraph = body.create_node(crate::Paragraph);
        body.insert_child_at(body.root(), 1, paragraph).unwrap();
        let c = body.create_node("c");
        body.insert_child_at(paragraph, 0, c).unwrap();
        let span = body.create_node(crate::Span::default());
        body.insert_child_at(paragraph, 1, span).unwrap();

        merge_leaves(&mut body, b, c);

        assert_eq!(body.text_of(b), "bc");
        assert_eq!(body.children_of(list), vec![item1, item2]);
        assert_eq!(body.children_of(item2), vec![b, span]);
        assert_eq!(body.children_of(body.root()), vec![list]);
    }

    #[test]
    fn test_merge_leaves_list_into_preceding_paragraph() {
        use crate::{List, ListItem};

        let mut body = ContentHandle::direct();

        let paragraph = body.create_node(crate::Paragraph);
        body.insert_child_at(body.root(), 0, paragraph).unwrap();
        let a = body.create_node("a");
        body.insert_child_at(paragraph, 0, a).unwrap();

        let list = body.create_node(List::default());
        body.insert_child_at(body.root(), 1, list).unwrap();

        let item1 = body.create_node(ListItem::default());
        body.insert_child_at(list, 0, item1).unwrap();
        let b = body.create_node("b");
        body.insert_child_at(item1, 0, b).unwrap();
        let span = body.create_node(crate::Span::default());
        body.insert_child_at(item1, 1, span).unwrap();

        let item2 = body.create_node(ListItem::default());
        body.insert_child_at(list, 1, item2).unwrap();
        let c = body.create_node("c");
        body.insert_child_at(item2, 0, c).unwrap();

        merge_leaves(&mut body, a, b);

        assert_eq!(body.text_of(a), "ab");
        assert_eq!(body.children_of(paragraph), vec![a, span]);
        assert_eq!(body.children_of(list), vec![item2]);
        assert_eq!(body.children_of(body.root()), vec![paragraph, list]);
    }
}
