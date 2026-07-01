Contient les modèles liés à un acte légal et les composants leptos.

# Structure
legal_act/
├─ src/
│  ├─ crdt.rs               # Implémentation des traits avec yrs (CRDT)
│  ├─ lib.rs
│  ├─ cursor.rs             # Définit le Cursor et le Selection
│  ├─ direct.rs             # Implémentation directe des traits
│  ├─ kind.rs
│  ├─ id.rs
│  ├─ traits/
│  │  ├─ mod.rs
│  │  ├─ node.rs            # Contient les traits relatifs aux noeuds du corps de l'acte
│  │  ├─ act.rs             # Contient les traits pour les différents actes légaux
│  │  ├─ review.rs          # Contient les traits pour lire, créer, supprimer les commentaires et les suivi de modifications
│  ├─ editor/
│  │  ├─ component.rs
│  │  ├─ state.rs
│  │  ├─ events.rs
│  │  ├─ content.rs
│  │  ├─ review.rs
│  │  ├─ mod.rs
├─ Cargo.toml

# Description

Il existe deux types d'actes légaux :
- Les actes légaux figés contenant
    - Métadonnées comprenant notamment :
        - L'identifiant de l'autorité 
        - Les dates relatives à la signature, l'entrée en vigueur
        - Les rétroliens vers les actes modifiant cet acte
        - Les liens vers les actes modifiés par cet acte
        - Des données métiers stockés sous forme de Knowledge Graph
    - Titre
    - Corps
- Les actes légaux en projet qui contient les élément d'un acte légal figé mais en plus 
    - Les commentaires relatives à une sélection traversante de tous les éléments de l'acte asocié à un utilisateur avec la 
        possibilité de résoudre ou de répondre de manière arborescente
    - Les notes de travail qui peuvent être privés ou publics 
    - Titre
    - Corps

Le corps contient les noeuds suivants :
- Root
- Visas 
- Considérants
- Sur 
- Titre
- LibelléTitre
- Section
- LibelléSection
- Chapitre 
- LibelléChapitre
- Article 
- LibelléArticle
- Annexe
- LibelléAnnexe
- Paragraphe
- Plain : Texte brut 
- Span : permet de mettre en gras, italique, souligné, barré une portion du texte.
- Table
- TableRow
- TableCell
- List
- ListItem

Les noeuds suivants : Paragraphe, Plain, Span, Table, TableRow, TableCell, List et ListItem sont désignés comme des noeuds de contenu.

# Exigences 

# Exigence de structure

- Il n'existe qu'un seul root (Root) à la racine du corps ;
- Le root ne peut être pas être supprimé.
- Root ne peut admettre comme enfant direct que des Visas, Considérants, Sur, Titre, Section, Chapitre, Annexe ou Article
- Les visas ne peuvent admettre comment enfant direct que des plains et des spans
- Les considérants ne peuvent admettre comment enfant direct que des plains et des spans 
- Les Titres ne peuvent admettre comme enfant direct que des chapitres, sections ou articles
- Les Chapitres ne peuvent admettre comme enfant direct que des sections ou articles
- Les annexes ne peuvent admettre comme enfant direct que des articles
- Les articles ne peuvent admettre comme enfant direct que des paragraphes, tableaux ou listes
- Les paragraphes ne peuvent admettre comme enfant direct que des plains ou des spans 
- Les tableaux ne peuvent admettre comme enfant direct que des lignes de tableaux (TableRow) ;
- Les lignes de tableau (TableRow) ne peuvent admettre comme enfant direct que des cellules (TableCell) ;
- Les cellules de tableau (TableCell) ne peuvent admettre comme enfant direct que des paragraphes, ou des listes
- Les listes ne peuvent admettre comme enfant direct que des ListItems
- Les ListItems ne peuvent admettre comme enfant direct que des plain ou spans.
- Les Libellé* ne peuvent admettre comme enfant direct que des plain ou des spans.
- Les visas sont toujours regroupés en premiers dans le root ;
- Les considérants sont toujours regroupés après les visas ;
- Le sur est toujours après les considérants ;
- Les annexes sont regroupés et toujours situés à la fin ;
- Seules les plains sont des feuilles.

Les traits doivent assurer la conformités à ces exigences structurelles.

# Exigence sur les fusions/divisions 

Les fusions et divisions ne marchent que pour les noeuds de contenu. Les fusions et divisions ne sont pas réalisées pour les autres noeuds et notamment la division récursive ascendante. 

- La fusion de deux noeuds (source vers target) du même type entraîne l'ajout des enfants du noeud source à la fin des enfants du noeud target.
- La fusion d'un paragraphe (source) dans autre paragraphe (target) ajoute les enfants de source à la fin des enfants de target.
- La fusion d'un paragraphe (source) dans une liste (target) ajoute les enfants de source à la fin des enfants du dernier ListItem ;
- La fusion d'une liste (source) dans un paragraphe (target) va ajouter les enfants du premier ListItem dans le paragraphe ; Si la liste est vide alors supprime le noeud.
- La fusion d'un ListItem (source) dans un autre ListItem (target) ajoute les enfants de source à la fin des enfants de target ;
- La division d'une feuille engendre la division récursive des parents.

Ces fonctions sont intégrées aux traits.

# Exigence sur les opérations de modification des noeuds

Les opérations suivantes ne marchent que pour les noeuds de contenu.

- Supprimer du texte dans un plain vide entraîne la suppression du noeud, si le parent n'a plus de noeud alors il est supprimé cela s'applique récursivement tant que le noeud est un noeud de contenu.
    - Si le parent non contenu est un Article, recrée un paragraphe > plain
    - Si le parent non contenu est un Libellé*, recrée un plain

# Exigence sur le contenu

- Les titres, chapitres, sections et articles sont numérotés ; 
- La numérotation fonctionne en imbriquée par rapport au parent ; elle est automatiquement mise à jour ;
- Les annexes sont numérotés ensemble et ne dépendent pas des autres siblings ;

### Ergonomie de l'éditeur

- Tout élément modifiable affiche une icône discrète (crayon) ou un contour pointillé au survol.
- Simple clic ou double clic sur un élément → passage en mode édition inline.
- `focusout` → enregistrement automatique de la modification.
- L'ajout ou la réorganisation de nœuds `LegalActContent` (titres, chapitres, articles…) se fait par des contrôles contextuels (bouton `+`, drag-and-drop) sans quitter le flux de lecture.

# Exigence sur l'interface de l'éditeur

L'édition d'un acte légal ne doit pas utiliser directement de textarea. Un textarea caché pourra être utilisé pour activer notamment les claviers virtuels.

L'édition doit être sans rupture avec l'affichage en lecture de sorte à ce qu'il suffise juste de cliquer dessus et de modifier automatiquement. Une fois le blur déclenché la modification est enregistrée dans le yrs::Doc.

Les éléments éditables sont les libellés des noeuds intermédiaires ou le contenu d'un article.

Un système de curseur et de sélection pour parcourir les noeuds feuilles (plain) sont mis en place. Le curseur pointe toujours vers un plain.

Une sélection est constituée d'un curseur de début (anchor) et d'un curseur de fin (focus), avec anchor < focus.

En mode collaboratif, les curseurs des autres utilisateurs sont affichés. 

Les opérations liées au curseur :
- Les flêches gauche et droit déplacent le curseur dans le plain, lorsqu'il est aux bornes, saute sur la feuille précédente ou suivante ;
- Les flêches haut et bas sautent vers la ligne du bas au sens géométrique ;
- Backspace : supprime le caractère précédent le curseur ;
- Delete : supprime le caractère après le curseur ;
- Ajout de texte (par exemple : frappe de touche): ajoute le texte à droite du curseur ;


