## Persistence (`storage`)

Crate dédié à la persistence des données applicatives. Le crate `server` est le seul consommateur autorisé à effectuer des requêtes SQL ; `storage` expose des ports/repositories typés, jamais de `sqlx::query` brut en dehors de ce crate.

### Stack technique

- `sqlx` avec driver Postgres (async, requêtes vérifiées à la compilation).
- Migrations versionnées (`sqlx::migrate!` ou équivalent) dans `storage/migrations/`.

### Erreurs

- Un enum `StorageError` (via `thiserror`) par domaine ou global, jamais de `Box<dyn Error>` en signature publique.
- Pas de `unwrap()`/`expect()` : toute erreur SQL ou de désérialisation est propagée avec `?`.

---

## Schéma de données

### Utilisateurs et groupes

- `users` : compte utilisateur (identifiant, email, statut actif/suspendu, métadonnées de profil). Un compte suspendu (`suspended_at`) n'est jamais supprimé.
- `groups` : hiérarchie parent/enfant auto-référencée (`parent_group_id`) — une entité correspond à un groupe possédant des sous-groupes.
- `user_groups` : table pivot N:N (`user_id`, `group_id`).
- La suppression d'un utilisateur ou d'un groupe doit **propager immédiatement** l'invalidation de toutes ses permissions actives et sessions en cours (cf. contrainte racine « Propagation des révocations »).


### Domain (domaine technique) et Intention

- `domains` : référentiel des domaines techniques d'un acte (ex. « Installation classée »), géré par les administrateurs. Porte `agent_context` : texte injecté en complément du prompt système de l'agent IA lorsqu'un projet appartient à ce domaine.
- `intentions` : rattachées à un domaine (`domain_id`, FK vers `domains`, `ON DELETE CASCADE`), configurables par les administrateurs. Portent elles aussi un `agent_context` injecté dans le prompt système. Seules les intentions du domaine d'un projet peuvent lui être associées.
- `legal_act_intentions` : table pivot N:N (`legal_act_id`, `intention_id`) — un projet peut avoir plusieurs intentions, ajoutées/retirées directement dans l'éditeur (voir `app::pages::project_intentions`).
- `legal_acts.domain_id` (FK vers `domains`) remplace l'ancienne notion d'« issuer » (signataire) : fixé à la création du projet, jamais modifié ensuite.

### Agent tool scopes (disponibilité des outils de l'agent par domaine)

- `agent_tool_scopes` : `(tool_name, domain_id)`, où `domain_id NULL` signifie une disponibilité globale de l'outil (ex. Légifrance) et `domain_id` renseigné réserve l'outil à ce domaine précis (ex. GéoRisques pour « Installation classée »). Un outil absent de cette table n'est disponible dans aucun domaine (moindre privilège). Configurable par les administrateurs (voir `agent::tools::CONFIGURABLE_TOOLS` pour le catalogue des outils concernés), consommé par `server::editor::ws` pour filtrer les outils enregistrés dans la boucle agentique.

### Identifiants (authentification par credentials)

- `credentials` : sous-table 1:1 de `users` (clé primaire = `user_id`, `FOREIGN KEY ... ON DELETE CASCADE`) portant le hash Argon2 du mot de passe (`password_hash`). Un utilisateur sans ligne `credentials` ne peut s'authentifier que via un provider OpenID Connect.
- Le hachage/vérification (Argon2, sel aléatoire embarqué dans le hash) est réalisé par `storage::credential` : le mot de passe en clair n'est jamais persisté ni renvoyé.

### Permissions

- `permissions` : triplet `(subject, resource, action)` où `subject` est soit `user_id` soit `group_id` (jamais les deux — contrainte `CHECK` exclusive), jamais de droit par défaut.
- `resource` modélise :
  1. `(type_ressource, identifiant_ressource)` — droit sur une ressource précise ;
  2. `(type_ressource, groupe_gestionnaire)` — droit sur toute ressource gérée par un groupe.
- Droits spéciaux `administrateur` et `super administrateur` représentés comme des actions réservées, jamais comme un champ booléen à part (cohérence avec le modèle triptyque).
- Les droits effectifs d'un utilisateur = union des droits primaires (`user_id`) et des droits dérivés de tous ses groupes (ascendance incluse).

### Authority (autorités administratives)

- `authorities` : référentiel des autorités administratives dans leur ensemble (ex. « DREAL », « Préfecture de la région Île-de-France », « DDPP »), indépendamment de tout groupe applicatif ou de tout acte.
- Champs : `nom` (correspond à `LegalActMeta::authority_name`, affiché dans le bloc-marque Marianne), `code` (identifiant stable, correspond à `LegalActMeta::autorite_id`), et les métadonnées d'affichage nécessaires au bloc-marque (logo, tutelle).
- `authorities` est un référentiel administré (CRUD réservé aux administrateurs), distinct du cycle de vie des projets d'arrêtés.

### Fournisseurs OpenID Connect

- `oidc_providers` : configuration par provider (issuer, client_id, endpoints, scopes, statut actif).
- Le `client_secret` est chiffré au repos (jamais stocké en clair) ; seul `server` détient la clé de déchiffrement via `SECRET_ENCRYPTION_KEY` (voir § « Secrets applicatifs chiffrés » ci-dessous, partagée avec les modèles IA et les intégrations externes).

### Secrets applicatifs chiffrés (modèles IA, intégrations externes)

Toutes les tables suivantes chiffrent leur secret au repos (AES-256-GCM, `shared::crypto`) avec la même clé `SECRET_ENCRYPTION_KEY` que `oidc_providers` ; seul `server` la détient et peut déchiffrer.

- `ai_models` : modèles de langage compatibles avec l'API de complétion de chat OpenAI (`base_url`, `model`, `api_key_encrypted`, `system_prompt`), gérables depuis `/admin/ai-models`. Au plus un modèle est `active` (index unique partiel `ai_models_single_active_idx`) : c'est celui utilisé comme moteur de l'agent « Marie » (voir `server::editor::ws::spawn_agent_run`). `system_prompt` est propre au modèle et s'ajoute en entête du prompt de base, avant les contextes de domaine et d'intentions (voir `server::editor::ws::build_agent_context`).
- `georisques_credentials` / `legifrance_credentials` : tables singleton (une ligne, `id` contraint à `1`) portant respectivement la clé API GéoRisques (optionnelle : l'API `v1` est accessible sans jeton) et le couple `client_id`/`client_secret` OAuth2 du portail PISTE Légifrance. Gérables depuis `/admin/integrations`. En l'absence de configuration Légifrance complète, les outils `legifrance_search`/`legifrance_fetch` restent indisponibles.

### Sessions

- `sessions` : sessions d'authentification (`id` opaque, `user_id`, `created_at`, `expires_at`), créées à la connexion (credentials ou OIDC) et supprimées à la déconnexion ou à l'expiration. `id` correspond à la valeur portée par le cookie de session côté client (cookie chiffré, cf. `server::auth`) — `storage` ne connaît que l'identifiant, jamais le cookie lui-même.
- `storage::session::delete_sessions_for_user` doit être appelé par `server` chaque fois qu'un utilisateur ou groupe est supprimé/suspendu, pour respecter la contrainte racine « Propagation des révocations ».

### Actes légaux — CRDT Yrs

Persistance en deux niveaux, alignée sur le fonctionnement `yrs` du crate `legal_act` (`encode_diff_v1` / `apply_update`) :

- `legal_act_updates` : journal append-only des mises à jour incrémentales (`update: bytea`, `legal_act_id`, `seq`, `created_at`, `author_id`). Chaque update WebRTC/serveur appliqué au `Doc` y est ajouté tel quel — aucune transformation.
- `legal_act_snapshots` : dernière version **consolidée** du document (`encode_state_as_update_v1` contre `StateVector::default()`), régénérée périodiquement par `worker` pour purger le journal d'updates devenu redondant.
- Invariant de lecture : reconstruire un `Doc` = charger le dernier snapshot puis rejouer les `legal_act_updates` postérieurs à ce snapshot (`seq > snapshot.seq`).
- La consolidation ne doit jamais supprimer un update tant que le snapshot correspondant n'est pas confirmé écrit (transaction atomique snapshot + purge).

### Configuration applicative

- `configurations` : table clé/valeur extensible (`key: TEXT PRIMARY KEY`, `value: JSONB`, `updated_at`, `updated_by`) — pas de schéma figé par paramètre, pour ne pas migrer la base à chaque nouveau réglage.
- Paramètres portés : tout paramètre global de configuration de l'application (quotas, feature flags, seuils de rate-limiting de l'API publique...). Le prompt système de l'agent IA `Marie` n'y transite pas : il est composé à partir du prompt de base (`server::editor::ws::AGENT_SYSTEM_PROMPT`), du `system_prompt` dédié du modèle actif (`ai_models`, voir § « Secrets applicatifs chiffrés ») et des contextes de domaine/intentions.
- Lecture mise en cache côté `server` ; écriture réservée aux administrateurs et **auditée** (cf. `audit_log`) car ces paramètres influencent le comportement de l'agent IA sans passer par une revue de code.

---

## Contraintes transverses

- Les invariants structurels (`ContentNode`, `LegalActNode`, `Appendix` en fin de liste) sont garantis en amont par `legal_act` ; `storage` ne fait que sérialiser/désérialiser, il ne valide pas la sémantique métier.
- Aucune vérification de permission ne doit être déléguée à `storage` : il fournit les données nécessaires à `server` pour trancher, mais ne décide jamais lui-même de l'autorisation.
- Toute action sensible (création/suppression d'utilisateur, modification de permission, accès à un acte) doit pouvoir être tracée par `server` via les données exposées ici (horodatage, identité, ressource) — `storage` fournit les tables `audit_log` nécessaires mais n'écrit pas la logique d'audit elle-même.
