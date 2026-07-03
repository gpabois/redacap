Editeur d'arrêtés préfectoraux
==============================

Application web collaborative de rédaction d'arrêtés préfectoraux à destination des inspecteurs des installations classées pour la protection de l'environnement (ICPE). L'outil gère l'ensemble du cycle de vie d'un acte légal : rédaction collaborative, workflow de validation, et génération de documents finaux.

# Modalités agentiques 

- Tu chargeras en priorité les Claude.md s'ils existent dans les crates qui sont ciblés
par la demande que je te soumets.

- Si tu ne trouves pas les réponses pour réaliser ma requête, tu pourras procéder
à des lectures directes dans code source.

- Privilégie la lecture du doc.rs des dépendances plutôt que la lecture directe.


- **Ne jamais utiliser `unwrap()` ou `expect()`** dans du code non-test. Propager les erreurs avec `?`.

- **Les invariants structurels** de `ContentNode` et `LegalActNode` doivent être vérifiés à la construction (constructeurs typés, pas de champs publics mutables directs).

- **L'API opaque** (`ContentHandle`, `LegalActHandle`) doit cacher complètement le mode direct vs Yrs : les composants Leptos ne doivent pas savoir dans quel mode ils opèrent.

- **WebRTC** : tout le trafic P2P est chiffré (DTLS-SRTP). Ne jamais transmettre de donnée sensible en clair.

- **Les `Appendix`** doivent toujours être repositionnés en fin de liste lors d'une insertion ; valider cet invariant dans les tests.

- **Les permissions** sont vérifiées côté serveur à chaque requête API, jamais uniquement côté client.

- **L'agent IA** ne valide jamais sans confirmation utilisateur (`ask_user`) pour les actions irréversibles (remplacement de section entière, modification de métadonnées critiques).

## Stack technique

| Domaine | Technologie |
|---|---|
| Langage | Rust (exclusif) |
| IHM | Leptos (SSR + hydratation) |
| CRDT | Yrs (port Rust de Yjs) |
| P2P | WebRTC chiffré |
| Styling | Tailwind CSS |
| Format de sortie | ODT, PDF |

# Structure
- server : gère la partie backend applicatif et notamment le SSR
- app : contient les données applicatifs utilisés par le frontend et le server
- frontend : gère la partie frontend applicatif, notamment l'hydratation
- agent : contient les fonctions de la boucle agentique `Marie`
- dsfr : contient les composants du système de design de l'état ainsi que des composants customisés, il doit être utilisé en priorité pour construire les composants leptos
- legal_act: contient les modèles et les composants leptos (éditeur) liés aux actes légaux
- render : permet la rendition ODT/PDF des arrêtés préfectoraux
- worker : utiliser pour réaliser des tâches longues de manière asynchrone comme la rendition d'odt/pdf, l'envoi de courriels, etc.
- content : contient les modèles et composants pour le corps des articles ;
- shared : contient des utilitaires servant un peu partout dont notamment le générateur d'identifiants

# Exigences

- Permettre l'édition collaborative d'arrêtés préfectoraux assistée par un agent IA ;
- Réaliser la rendition des arrêtés en ODT/PDF ;
- Permettre l'archivage et la recherche d'arrêtés préfectoraux ;
- Permettre l'accès par API / MCP des arrêtés préfectoraux ;


## Modèle de permissions

```
Utilisateur ←→ Groupe (N:N)
Permission → (Utilisateur | Groupe) + Ressource + Action
```

- Les permissions sont assignables directement à un utilisateur ou à un groupe.
- Les droits d'un utilisateur = union de ses droits directs et des droits de tous ses groupes.
- Principe du moindre privilège : aucun droit par défaut.


---

## Édition collaborative

### Mode de fonctionnement

L'éditeur supporte deux modes :
- **Mode direct** : modifications locales immédiates, sans CRDT.
- **Mode Yrs** : édition collaborative via CRDT Yrs sur canal WebRTC chiffré.

Le mode est sélectionné à l'initialisation et transparent pour les composants UI.

### Droits d'édition

- Le créateur d'un projet a automatiquement tous les droits d'édition.
- L'octroi de droits se fait par :
  - désignation explicite d'un membre ou d'un groupe
  - lien d'invitation à usage unique avec expiration

### Chat collaboratif

- Canal de chat intégré entre les rédacteurs actifs.
- Implémenté via un `yrs::Doc` dédié (tableau de messages CRDT).
- Persistance : les messages sont conservés dans l'historique du projet.

### Ergonomie de l'éditeur

- Tout élément modifiable affiche une icône discrète (crayon) ou un contour pointillé au survol.
- Simple clic ou double clic sur un élément → passage en mode édition inline.
- `focusout` → enregistrement automatique de la modification.
- L'ajout ou la réorganisation de nœuds `LegalActContent` (titres, chapitres, articles…) se fait par des contrôles contextuels (bouton `+`, drag-and-drop) sans quitter le flux de lecture.


## Workflow de validation

```
Rédaction → [Vérification (1..N)] → Approbation → Génération ODT
```

- **Vérificateurs** : un ou plusieurs membres désignés explicitement ou via un groupe.
- **Approbateur** : un membre ou un groupe ; un seul approbateur suffit.
- Chaque étape émet une notification (in-app + email configurable).
- Un vérificateur peut demander des corrections (retour en rédaction avec commentaire).
- Une fois le projet approuvé, il passe en statut `Finalisé` : lecture seule, génération ODT disponible.
- La génération PDF est disponible à tout moment (aperçu non-officiel).


---

## Pages de l'application

### Domaine principal

| Route | Page |
|---|---|
| `/login` | Authentification OpenID Connect |
| `/logout` | Déconnexion + invalidation de session |
| `/account` | Paramètres du compte utilisateur |
| `/projects` | Liste des projets d'arrêtés |
| `/projects/new` | Création d'un projet |
| `/projects/:id` | Édition collaborative d'un projet |
| `/projects/:id/workflow` | Suivi du workflow de validation |

### Domaine administrateur (`/admin`)

| Route | Page |
|---|---|
| `/admin/users` | Gestion des comptes utilisateurs et permissions |
| `/admin/groups` | Gestion des groupes |
| `/admin/oidc` | Configuration des providers OpenID Connect |
| `/admin/audit` | Journal d'audit des accès et actions sensibles |

---

## Variables d'environnement

```env
DATABASE_URL=                  # URL PostgreSQL
SESSION_SECRET=                # Secret HMAC pour les cookies de session (≥32 bytes)
WEBRTC_STUN_SERVERS=           # URLs des serveurs STUN (JSON array)
LEGIFRANCE_API_KEY=            # Clé API Légifrance
GEORISQUES_API_KEY=            # Clé API GéoRisques (optionnel)
OIDC_PROVIDERS=                # Configuration JSON des providers autorisés
AI_AGENT_ENDPOINT=             # Endpoint du modèle IA (compatible OpenAI API)
AI_AGENT_API_KEY=              # Clé API du modèle IA
RENDER_LIBREOFFICE_PATH=       # Chemin vers soffice pour la conversion PDF (optionnel)
```

---

## Commandes courantes

```bash
# Build complet
cargo build --workspace

# Lancer le serveur en développement
cargo run -p server

# Build WASM du client
cargo build -p web_client --target wasm32-unknown-unknown

# Tests
cargo test --workspace

# Vérification des types sans build
cargo check --workspace

# Linting
cargo clippy --workspace -- -D warnings

# Formatage
cargo fmt --all
```

---

### Exigences complémentaires (état de l'art)

- **MFA** : support TOTP (via l'IdP OpenID Connect) encouragé, non bloquant si l'IdP ne le propose pas.
- **Audit log** : toute action sur une ressource sensible (création/suppression d'utilisateur, modification de permission, accès à un acte) est tracée avec horodatage, identité et IP.
- **Suspension de compte** : un administrateur peut suspendre un compte sans le supprimer ; les sessions actives sont invalidées immédiatement.
- **Invitation par lien** : lien à usage unique avec expiration configurable (défaut : 48 h).
- **Propagation des révocations** : la suppression d'un utilisateur ou d'un groupe invalide immédiatement toutes ses permissions actives.
- **Séparation admin / utilisateur** : les comptes administrateurs sont distincts des comptes métier.