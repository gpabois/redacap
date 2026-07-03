Serveur de Redac'Ap

# Stack technique

- Axum comme serveur HTTP ;
- sqlx avec un driver Postgres ;

# Fonctions techniques

- Générer les différentes pages Web de l'outil (par SSR)
- Fournir une API privée au frontend via les ServerFunctions et les Websockets
- Gérer la persistence des données
- Gérer l'authentification via des OpenID providers
- Fournir une API public et notamment les Webhooks

# Fonctions métiers
- Gérer l'édition d'actes légaux réalisée de manière collaborative
- Gérer le circuit de validation 
- Gérer la rendition ODT/PDF des projets d'arrêtés préfectoraux
- Gérer l'envoi à d'autres services
- Gérer l'archivage et la GED des documents

# Exigences

## Agent AI

1. Limite les tokens utilisés par utilisateur suivant un système de session d'une durée de 4 heures ;
2. Gérer les boucles agentiques ;
3. 

# Utilisateur et groupe

- Les utilisateurs peuvent être membre d'un ou plusieurs groupes
- Les groupes peuvent gérer d'autres groupes (relation parent <-> enfant)
- Une entité correspond à un groupe possédant des sous-groupes 

## Authentification

1. L'authentification peut se faire par l'un des fournisseurs OpenID enregistrés dans l'application 
2. L'authentification peut se faire par credentials directement


## Autorisation

- Les droits sont lié au tryptique (utilisateur, action, ressource) ;
- Les droits peuvent être attribués à un utilisateur (droit primaire) ;
- Les droits peuvent être attribués à un groupe (droit dérivé) ;
- Une entité hérite automatiquement de l'ensemble des droits de ses descendants ;
- L'autorisation pour un utilisateur se fait en tenant compte des droits primaires et dérivés
- Un droit spécial dit `administrateur` permet d'effectuer toutes les actions sauf de retirer les droits à un autre administrateur ou à un `super administrateur`
- Le droit spécial `super administrateur` permet d'effectuer toutes les actions y compris assigner et retirer les droits `administrateur` et `super administrateur`
- Ressource peut s'entendre comme :
    1. (type ressource, identifiant d'une ressource)
    2. Mais aussi des conditions spéciales :
        a. (type ressource, ressource gérée par un groupe)
        

# API public

- Le serveur fournit une API public RESTful 
- L'accès à cet API se fait par une clé API émise par une entité ;
- Les droits de la clé API ne peuvent excéder les droits assignés à l'entité ;
- Les droits de la clé API peuvent être plus restrictives que les droits de l'entité ;
- Les requêtes de la clé API sont limitées dans le temps ; tu peux choisir l'unité temporelle pour assurer un bon compromis entre protection de la QoS et fourniture des données ;
- Les requêtes sont auditables et sont loggées et conservées pendant un temps raisonnable ;

# Model Context Protocol

- Le serveur fournir un MCP à destination d'agents 
- L'accès au MCP se fait par une clé API dans les mêmes modalités que [`API public`]
- Le MCP fournit les entrées suivantes :
    - Recherche d'un arrêté préfectoral suivant plusieurs vecteurs de recherche ;
    - Lecture du contenu d'un arrêté 

## Sécurité

1. Tous les points d'entrées sont protégées, l'autorisation est toujours vérifiée par rapport aux droits de l'utilisateur émetteur de l'action qu'ils soient primaire (lié à l'utilisateur) ou dérivé (lié à un ou plusieurs groupes pour lesquelles l'utilisateur est membre) ; pour les API pulics, les permissions sont vérifiées par rapport à la clé API

dans le cas des API publics l'utilisateur peut restreindre les permissions par clé API mais ne peut pas obtenir plus de permissions que l'utilisateur émetteur des clés.

2. L'authentification pour l'affichage d'une page par SSR ou par API privée (ServerFn ou Websocket) se fait via un Cookie de Session opaque d'une durée de vie de 24h.

- Stockage de session : cookie HttpOnly + SameSite=Strict, durée configurable.

3. L'authentification pour l'API public ou par SSR se fait par clé API