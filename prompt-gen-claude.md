Génère un Claude.md pour un projet d'application web d'édition d'arrêtés préfectoraux à destination des inspecteurs des installations classées.

# Stack technique

1. Le projet est programmé exclusivement en Rust. 
2. Leptos pour la partie IHM ;
3. Yrs sera utilisé pour réaliser du CRDT.
4. Le P2P passera par WebRTC chiffré
5. Tailwind pour la partie styling

# Convention 

Les variables sont codées en snake_case, les noms de struct en PascalCase.
Les fonctions doivent être simples.

Tu ajouteras d'autres exigences pour la génération d'un code Rust élégant, fiable, simple et maintenable. 

# Structure

Il contiendra un crate workspace dôté des membres suivants :
- App qui contiendra les composants et pages Leptos qui servira à Server et WebClient ;
- Server qui servira à la fois un API et du SSR (Leptos) ;
- Content qui contient le modèle Content et le composant ContentEditor et qui sert de dépendance à LegalAct pour le contenu des articles ;
- LegalAct qui contient les modèles pour créer des projets et des version définitives ;
- Render qui contient les fonctions permettant de générer des ODT et des PFD à partir de LegalAct ;
- WebClient qui servira à réhydrater le SSR côté navigateur ;

Tu proposeras une structure à partir des différentes exigences ci-après.

# Typologie d'arrêtés préfectoraux

- Mise en demeure ;
- Sanction administrative ;
- Arrêté d'autorisation environnementale ;
- Arrêté d'enregistrement ;
- Arrêté complémentaire
- Arrêté de mesure d'urgence.


# Exigences

Tu proposeras en plus des exigences fonctionnelles, des exigences techniques.

## Modèle 

Exigence: Un LegalAct doit posséder les éléments suivants :
- Identifiant de l'autorité émettrice de l'acte, c'est un identifiant opaque en base de données ;
- Un titre ;
- Les visas ;
- Les considérants ;
- Sur proposition
- Contenu (LegalActContent)

Exigence: Un LegalActContent est une structure arborescente possédant les noeuds intermédiaires suivants :
- Title possédant optionellement un libellé ;
- Chapter possédant optionellement un libellé ;
- Section possédant optionellement un libellé ;
- Appendix possédant optionellement un libellé ;

Exigence: Un LegalActContent possède le noeud terminal suivant :
- Article possédant un Content et optionellement un libellé 

Exigence : Les annexes doivent toujours être situés à la fin. 
Exigence : Les numérotations doivent être imbriquées, ils peuvent être modifier comme un ListMarker HTML (i, a, A, I, 1, ...)

Exigence : Un LegalAct doit pouvoir être possible directement ou en tant que MapRef d'un yrs::Doc ; 

Exigence : Un Content est une structure arborescente possédant les noeuds intermédiaires suivants :
- Paragraph ;
- Span : pour mettre du texte en gras, italique, souligné ou barré ;
- Table ;
- TableRow ;
- TableCell ;
- List ;
- ListItem ;

Exigence : Un content posséde un noeud terminal suivant : Plain qui contient uniquement du texte.
Exigence : Les listes peuvent posséder un ListMarker HTML ordonnés (i, a, A, I) ou non (disc, ...)
Exigence : Aucun élément intermédiaire ne peut être un élément terminal ;
Exigence : Un Table ne peut avoir que pour enfants direct que des TableRows ;
Exigence : Un TableRow ne peut avoir que pour enfants directs que des TableCells ;
Exigence : Un TableCell ne peut avoir que des Span ou des Plain ;
Exigence : Un Paragraph ne peut avoir que des Span ou des Plain ;
Exigence : Un List ne peut avoir que des ListItems ;
Exigence : Un ListItem ne peut avoir que des Plain ou des Span.
Exigence : Un Content doit pouvoir être possible directement ou en tant que MapRef d'un yrs::Doc ; 
Exigence : Une API opaque doit abstraire les cas directs des cas CRDT ;

# Ergonomie 

Exigence : Les éléments modifiables doivent posséder une petite icône ou un feedback permettant à l'utilisateur de savoir que l'élément est modifiable ;
Exigence : Le simple clic ou double clic sur un élément doit le rendre modifiable, le focus out doit enregistrer la modification.

# Génération des ID

Exigence : Les ID doivent être générés par deux ID : la première est un ID de session (64 bits) et la seconde un ID issu d'un pseudo-rng avec graine aléatoire.

# Utilisateurs et groupes

Exigence : L'outil doit proposer des groupes utilisateurs ;
Exigence : Les permissions sont assignables directement aux utilisateurs ou aux groupes d'utilisateurs.
Exigence : Les utilisateurs doivent pouvoir s'authentifier par OpenID connect ;
Exigence : Seul les providers autorisés dans le panneau administrateur sont possibles ;

Tu proposeras éventuellement d'autres exigences qui sont l'état de l'art en matière de gestion des utilisateurs et des groupes.

## Edition de projets d'actes légaux

Exigence : L'édition doit être collaboratif via CRDT.
Exigence : Un chat doit être possible entre les différents rédacteurs, le chat utilisera yrs.
Exigence : Un agent IA est mis à disposition des acteurs pour rédiger tout ou partie de l'arrêté ou pour écrire et compléter les métadonnées ;
Exigence : L'édition des projets doit être uniquement possible à ceux qui ont le droit. Le créateur du projet a automatiquement les droits ;
Exigence : L'octroi de droit d'édition peut se faire soit par désignation explicite d'un membre ou d'un groupe, soit par un lien d'invitation.

Exigence : L'éditeur doit pouvoir fonctionner en mode direct ou yrs.
Exigence : L'édition des libellés de noeud ou l'ajout d'un noeud du LegalActContent doit être simple, fluide et intuitif. 

## Edition du Content

Exigence : L'éditeur de Content doit être simple, esthétique et intuitif.
Exigence : Il doit aussi bien marcher s'il est intégré comme éditeur d'un article que comme éditeur simple.
Exigence : L'éditeur doit aussi bien marcher si le content est géré par Yrs dans un ActeLegal géré par Yrs ou bien directement.


## Workflow de validation

Exigence : Le projet une fois finalisée, doit passer par un système de Workflow avec un ou plusieurs vérificateurs, et un approbateur ;
Exigence : Les vérificateurs ou approbateur peuvent aussi être un membre quelconque d'un groupe si le groupe est désigné comme tel ;
Exigence : Une fois le projet validé, un ODT doit pouvoir être généré.


## Agent IA pour l'édition d'arrêtés

Exigence : L'agent IA doit, par boucle agentique, permettre de compléter un arrêté en utilisant notamment les outils suivants :
- Requête auprès de Légifrance par API ;
- Demander des précisions ou confirmer des prises de décisions ;
- Demander éventuellement des documents externes ;
- Récupérer des données depuis les métadonnées contextuelles de l'acte légal.

Tu proposeras d'autres outils pertinents.

## Métadonnées contextuelles

Exigence : Les métadonnées peuvent être notamment :
- Données administratives d'une installation classée 
    - Code AIOT, rubrique ICPE et/ou IOTA et les paramètres associées
    - Les émissaires atmosphériques ;
    - Les points de rejets dans l'eau ;
    - Les installations de combustion ;
    - La gestion des déchets ;
    - La gestion des substances Seveso ;

Exigence : Les métadonnées doivent être dynamiques pour s'adapter à tous les cas.

# App

Le crate contient les pages suivantes par domaine

## Domaine principal 
- Login / Logout ;
- Gestion des paramètres de compte ;
- Edition d'un projet d'acte légal ;

## Domaine administrateur
- Edition des comptes utilisateurs pour octroyer des droits ;
- Gestion des providers OpenID Connect ; 
- Audit des accès
