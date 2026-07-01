# CLAUDE.md — Éditeur d'arrêtés préfectoraux (ICPE)

Application web collaborative de rédaction d'arrêtés préfectoraux à destination des inspecteurs des installations classées pour la protection de l'environnement (ICPE). L'outil gère l'ensemble du cycle de vie d'un acte légal : rédaction collaborative, workflow de validation, et génération de documents finaux.

---

## Stack technique

| Domaine | Technologie |
|---|---|
| Langage | Rust (exclusif) |
| IHM | Leptos (SSR + hydratation) |
| CRDT | Yrs (port Rust de Yjs) |
| P2P | WebRTC chiffré |
| Styling | Tailwind CSS |
| Format de sortie | ODT, PDF |

---

## Structure du workspace

```
Cargo.toml              # workspace root
├── crates/
│   ├── app/            # Composants et pages Leptos (partagé SSR + WebClient)
│   ├── server/         # Serveur HTTP : API REST + SSR Leptos
│   ├── content/        # Modèle Content + composant ContentEditor
│   ├── legal_act/      # Modèles LegalAct, LegalActContent, types d'arrêtés
│   ├── render/         # Génération ODT et PDF depuis LegalAct
│   └── web_client/     # Point d'entrée WASM — réhydratation côté navigateur
```


---

## Génération des identifiants

Chaque identifiant est composé de deux parties :

```rust
pub struct EntityId {
    pub session_id: SessionId,  // u64, unique par session serveur
    pub local_id: LocalId,      // u64, issu d'un pseudo-RNG à graine aléatoire
}

pub struct SessionId(u64);
pub struct LocalId(u64);
```

- `SessionId` : généré une fois au démarrage du serveur (ou de la session WASM), stocké en mémoire.
- `LocalId` : généré par un PRNG initialisé avec une graine aléatoire (`rand::thread_rng()`).
- La combinaison garantit l'unicité sans coordination centralisée.

---

## Gestion des utilisateurs et des groupes

### Authentification

- OpenID Connect exclusivement.
- Seuls les providers configurés dans le panneau administrateur sont autorisés.
- Stockage de session : cookie HttpOnly + SameSite=Strict, durée configurable.
- Support du refresh token avec rotation (token reuse detection).

### Modèle de permissions

```
Utilisateur ←→ Groupe (N:N)
Permission → (Utilisateur | Groupe) + Ressource + Action
```

- Les permissions sont assignables directement à un utilisateur ou à un groupe.
- Les droits d'un utilisateur = union de ses droits directs et des droits de tous ses groupes.
- Principe du moindre privilège : aucun droit par défaut.

### Exigences complémentaires (état de l'art)

- **MFA** : support TOTP (via l'IdP OpenID Connect) encouragé, non bloquant si l'IdP ne le propose pas.
- **Audit log** : toute action sur une ressource sensible (création/suppression d'utilisateur, modification de permission, accès à un acte) est tracée avec horodatage, identité et IP.
- **Suspension de compte** : un administrateur peut suspendre un compte sans le supprimer ; les sessions actives sont invalidées immédiatement.
- **Invitation par lien** : lien à usage unique avec expiration configurable (défaut : 48 h).
- **Propagation des révocations** : la suppression d'un utilisateur ou d'un groupe invalide immédiatement toutes ses permissions actives.
- **Séparation admin / utilisateur** : les comptes administrateurs sont distincts des comptes métier.

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

---

## Agent IA

L'agent opère par boucle agentique (ReAct) et dispose des outils suivants :

| Outil | Description |
|---|---|
| `legifrance_search` | Recherche dans la base Légifrance (textes législatifs, jurisprudence) via API officielle |
| `legifrance_fetch` | Récupère le contenu complet d'un texte par identifiant |
| `ask_user` | Pose une question ou demande une confirmation à l'inspecteur |
| `request_document` | Demande un document externe à l'utilisateur (upload) |
| `read_metadata` | Lit les métadonnées contextuelles de l'acte en cours |
| `write_metadata` | Met à jour les métadonnées contextuelles |
| `fill_section` | Remplit ou complète un nœud `LegalActContent` (article, considérant, visa…) |
| `georisques_query` | Interroge l'API GéoRisques pour les données d'une installation |
| `icpe_query` | Interroge les bases ICPE/AIOT pour les données administratives d'un établissement |
| `generate_numbering` | Recalcule la numérotation des nœuds après modification structurelle |
| `validate_structure` | Vérifie que l'acte respecte les invariants structurels avant génération |

L'agent peut composer ces outils en séquence pour rédiger tout ou partie d'un arrêté, compléter les visas réglementaires, ou vérifier la conformité des seuils ICPE.

---

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

## Génération de documents (`render`)

- **ODT** : format de sortie officiel, généré via des templates ODT (bibliothèque `lopdf` ou équivalent Rust pour la structure XML ODF).
- **PDF** : rendu via conversion ODT→PDF (LibreOffice headless en subprocess, ou bibliothèque Rust native).
- Les fonctions de rendu sont **pures** : `fn render_odt(act: &LegalAct) -> Result<Vec<u8>, RenderError>`.
- Aucun I/O dans le crate `render` : les appels réseau/filesystem se font dans `server`.

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

## Points d'attention pour Claude

- **Ne jamais utiliser `unwrap()` ou `expect()`** dans du code non-test. Propager les erreurs avec `?`.
- **Les invariants structurels** de `ContentNode` et `LegalActNode` doivent être vérifiés à la construction (constructeurs typés, pas de champs publics mutables directs).
- **L'API opaque** (`ContentHandle`, `LegalActHandle`) doit cacher complètement le mode direct vs Yrs : les composants Leptos ne doivent pas savoir dans quel mode ils opèrent.
- **WebRTC** : tout le trafic P2P est chiffré (DTLS-SRTP). Ne jamais transmettre de donnée sensible en clair.
- **Les `Appendix`** doivent toujours être repositionnés en fin de liste lors d'une insertion ; valider cet invariant dans les tests.
- **Les permissions** sont vérifiées côté serveur à chaque requête API, jamais uniquement côté client.
- **L'agent IA** ne valide jamais sans confirmation utilisateur (`ask_user`) pour les actions irréversibles (remplacement de section entière, modification de métadonnées critiques).
