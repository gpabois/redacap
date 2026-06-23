use core::panic;
use std::{default::Default, ops::Deref};
use crate::{model::{Article, AuthorityId}, utils::{use_id}};

pub trait LegalActModel {
    type Body: LegalActBodyModel;

    fn set_authority(&mut self, autorité: AuthorityId);
    fn try_authority(&self) -> &Option<AuthorityId>; 

    fn title(&self) -> &str;
    fn set_title<S: ToString>(&mut self, titre: S);

    fn add_considering<S: ToString>(&mut self, contenu: S);
    fn set_considering(&mut self, id: &ConsideringId, contenu: String);
    fn iter_considerings(&self) -> impl Iterator<Item=&Considering>;
    
    fn add_visa<S: ToString>(&mut self, contenu: S);
    fn set_visa(&mut self, id: &VisaId, contenu: String);
    fn iter_visas(&self) -> impl Iterator<Item=&Visa>;

    fn borrow_body(&self) -> &Self::Body;
    fn borrow_mut_body(&mut self) -> &mut Self::Body;
}

pub enum Offset {
    Head,
    Tail,
    Before(usize),
    After(usize)
}

pub trait LegalActBodyModel {
    fn root(&self) -> LegalActeNodeId;
    fn borrow_node(&self, id: LegalActeNodeId) -> Option<&LegalActNode>;
    fn borrow_mut_node(&mut self, id: LegalActeNodeId) -> Option<&mut LegalActNode>;
    fn children_of(&self, id: LegalActeNodeId) -> impl Iterator<Item=LegalActeNodeId>;
    fn parent_of(&self, id: LegalActeNodeId) -> Option<LegalActeNodeId>;

    fn append_child(&mut self, to: LegalActeNodeId, data: LegalActNodeData);
    fn r#move(&mut self, node: LegalActeNodeId, to: LegalActeNodeId, offset: Offset);
    fn compute_numerotation(&mut self, node: LegalActeNodeId);
}

#[derive(Default)]
pub struct LegalActProject {
    authority_id: Option<AuthorityId>,
    title: String,
    visas: Vec<Visa>,
    considerings: Vec<Considering>,
    body: LegalActBody
}

impl LegalActModel for LegalActProject {
    type Body = LegalActBody;

    fn set_authority(&mut self, autorité: AuthorityId) {
        self.authority_id = Some(autorité)
    }

    fn try_authority(&self) -> &Option<AuthorityId> {
        &self.authority_id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn set_title<S: ToString>(&mut self, titre: S) {
        self.title = titre.to_string();
    }

    fn add_considering<S: ToString>(&mut self, contenu: S) {
        let new_id = use_id();
        let id = ConsideringId(new_id());
        self.considerings.push(Considering { id, contenu: contenu.to_string() });
    }

    fn set_considering(&mut self, id: &ConsideringId, contenu: String) {
        let id = id.clone();
        let Some(considering) = self.considerings.iter_mut().find(|c| c.id == id) else { return };
        considering.contenu = contenu;
    }

    fn iter_considerings(&self) -> impl Iterator<Item=&Considering> {
        self.considerings.iter()
    }

    fn add_visa<S: ToString>(&mut self, contenu: S) {
        let new_id = use_id();
        let id = VisaId(new_id());
        self.visas.push(Visa { id, contenu: contenu.to_string() })
    }

    fn set_visa(&mut self, id: &VisaId, contenu: String) {
        let id = id.clone();
        let Some(visa) = self.visas.iter_mut().find(|c| c.id == id) else { return };
        visa.contenu = contenu;
    }

    fn iter_visas(&self) -> impl Iterator<Item=&Visa> {
        self.visas.iter()
    }

    fn borrow_body(&self) -> &Self::Body {
        &self.body
    }
    
    fn borrow_mut_body(&mut self) -> &mut Self::Body {
        &mut self.body
    }
}


#[derive(Hash, Clone, PartialEq, Eq)]
pub struct Visa {
    pub id: VisaId,
    pub contenu: String
}
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct VisaId(String);

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Considering {
    pub id: ConsideringId,
    pub contenu: String
}

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct ConsideringId(String);


pub struct LegalActBody {
    arena: indextree::Arena<LegalActNodeData>,
    pub racine: LegalActeNodeId
}

impl LegalActBodyModel for LegalActBody {
    fn root(&self) -> LegalActeNodeId {
        self.racine
    }

    fn borrow_node(&self, id: LegalActeNodeId) -> Option<&LegalActNode> {
        unsafe {std::mem::transmute(self.arena.get(id.0))}
    }

    fn borrow_mut_node(&mut self, id: LegalActeNodeId) -> Option<&mut LegalActNode> {
        unsafe {std::mem::transmute(self.arena.get_mut(id.0))}
    }

    fn append_child(&mut self, to: LegalActeNodeId, data: LegalActNodeData) {
        let node = to.0.append_value(data, &mut self.arena);
        self.compute_numerotation(LegalActeNodeId(node));
    }

    fn children_of(&self, id: LegalActeNodeId) -> impl Iterator<Item=LegalActeNodeId> {
        id.0.children(&self.arena).map(LegalActeNodeId)
    }

    fn parent_of(&self, id: LegalActeNodeId) -> Option<LegalActeNodeId> {
        id.0.parent(&self.arena).map(LegalActeNodeId)
    }
    
    fn r#move(&mut self, node: LegalActeNodeId, to: LegalActeNodeId, offset: Offset) {
        node.0.detach(&mut self.arena);
        
        match offset {
            Offset::Head => to.0.prepend(node.0, &mut self.arena),
            Offset::Tail => to.0.append(node.0, &mut self.arena),
            Offset::Before(index) => {
                let Some(sibling) = to.0.children(&self.arena).nth(index) else {
                    to.0.append(node.0, &mut self.arena);
                    self.compute_numerotation(node);
                    return;
                };

                sibling.insert_before(node.0, &mut self.arena);
            },
            Offset::After(index) => {
                let Some(sibling) = to.0.children(&self.arena).nth(index) else {
                    to.0.append(node.0, &mut self.arena);
                    self.compute_numerotation(node);
                    return;
                };

                sibling.insert_after(node.0, &mut self.arena);
            }
            
        }

        self.compute_numerotation(node);
        
        
    }

    fn compute_numerotation(&mut self, node: LegalActeNodeId) {
        let order = node.0.preceding_siblings(&self.arena).count();
        let mut numerotation = vec![];
        if let Some(parent) = node.parent(self) {
            numerotation.extend_from_slice(parent.get(self).numerotation());
        }
        numerotation.push(order as u32);

        node.get_mut(self).set_numerotation(numerotation);

        let children = node.children(self).collect::<Vec<_>>();
        children.into_iter().for_each(|node| self.compute_numerotation(node));
    }
}

impl std::default::Default for LegalActBody {
    fn default() -> Self {
        let mut arène: indextree::Arena<LegalActNodeData> = indextree::Arena::<LegalActNodeData>::new();
        let racine = LegalActeNodeId(arène.new_node(LegalActNodeData::Body));

        Self { arena: arène, racine }
    }
}

pub struct LegalActNode(indextree::Node<LegalActNodeData>);

impl AsRef<LegalActNodeData> for LegalActNode {
    fn as_ref(&self) -> &LegalActNodeData {
        self.0.get()
    }
}

impl Deref for LegalActNode {
    type Target = LegalActNodeData;

    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

impl std::ops::DerefMut for LegalActNode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.get_mut()
    }
}


impl LegalActNode {
    pub fn as_mut_article(&mut self) -> &mut Article {
        match self.0.get_mut() {
            LegalActNodeData::Article(article) => article,
            _ => panic!("not an article")
        }   
    }

    pub fn as_ref_article(&self) -> &Article {
        match self.0.get() {
            LegalActNodeData::Article(article) => article,
            _ => panic!("not an article")
        }
    }
}

#[derive(Hash, Clone, Copy, PartialEq, Eq)]
pub struct LegalActeNodeId(indextree::NodeId);

impl LegalActeNodeId {
    pub fn get<Body: LegalActBodyModel>(self, body: &Body) -> &LegalActNode {
        body.borrow_node(self).unwrap()
    }

    pub fn get_mut<Body: LegalActBodyModel>(self, body: &mut Body) -> &mut LegalActNode {
        body.borrow_mut_node(self).unwrap()
    }

    pub fn move_up<Body: LegalActBodyModel>(self, body: &mut Body) {
        
    }

    pub fn append_child<N, Body: LegalActBodyModel>(self, noeud: N, body: &mut Body) where LegalActNodeData: From<N> {
        body.append_child(self, LegalActNodeData::from(noeud));
    }

    pub fn parent<Body: LegalActBodyModel>(self, body: &mut Body) -> Option<LegalActeNodeId> {
        body.parent_of(self)
    }

    pub fn children<'body, Body: LegalActBodyModel>(self, body: &'body Body) -> impl Iterator<Item=LegalActeNodeId> + 'body {
        body.children_of(self)
    }
}


pub struct NoeudIntermédiaire {
    pub label: String,
}

#[derive(Clone)]
pub struct Annex {
    id: String,
    label: String,
    numerotation: Vec<u32>
}

impl Annex {
    pub fn new<S: ToString>(id: String, label: S) -> Self {
        let label = label.to_string();
        Self { id, label, numerotation: Vec::default() }
    }
}

#[derive(Clone)]
pub struct Chapter {
    id: String,
    label: String,
    numerotation: Vec<u32>
}

#[derive(Clone)]
pub struct Section {
    id: String,
    label: String,
    numerotation: Vec<u32>
}

#[derive(Clone)]
pub enum LegalActNodeData {
    Body,
    Annex(Annex),
    Chapter(Chapter),
    Section(Section),
    Article(Article),
    Paragraph,
    List,
    Table
}

impl LegalActNodeData {
    pub fn set_numerotation(&mut self, numerotation: Vec<u32>) {
        use LegalActNodeData::*;
        match self {
            Annex(annex) => annex.numerotation = numerotation,
            Chapter(chapter) => chapter.numerotation = numerotation,
            Section(section) => section.numerotation = numerotation,
            Article(article) => article.numerotation = numerotation,
            _ => {}
        }
    }
    pub fn numerotation(&self) -> &[u32] {
        use LegalActNodeData::*;

        match self {
            Body => &[],
            Annex(annex) => &annex.numerotation,
            Chapter(chapter) => &chapter.numerotation,
            Section(section) => &section.numerotation,
            Article(article) => &article.numerotation,
            Paragraph => &[],
            List => &[],
            Table => &[]
        }
    }
}

impl From<Annex> for LegalActNodeData {
    fn from(value: Annex) -> Self {
        Self::Annex(value)
    }
}

impl From<Article> for LegalActNodeData {
    fn from(value: Article) -> Self {
        Self::Article(value)
    }
}

pub struct Tableau {

}

pub struct CelluleTableau {

}