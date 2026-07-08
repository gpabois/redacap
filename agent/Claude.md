## Agent IA

L'agent opère par boucle agentique (ReAct) et dispose des outils suivants :

| Outil | Description |
|---|---|
| `legifrance_search` | Recherche dans la base Légifrance (textes législatifs, jurisprudence, CNIL, KALI, ACCO, circulaires...) via API officielle PISTE v2.4.2 (`/search`) |
| `legifrance_fetch` | Récupère le contenu complet d'un texte ou d'un article par identifiant, via la route `/consult/*` adaptée à son fonds (`legiPart`, `code`, `juri`, `jorf`, `getArticle`, `cnil`, `acco`, `kaliText`, `kaliArticle`, `circulaire`) |
| `ask_user` | Pose une question ou demande une confirmation à l'inspecteur |
| `request_document` | Demande un document externe à l'utilisateur (upload) |
| `read_metadata` | Lit les métadonnées contextuelles de l'acte en cours |
| `write_metadata` | Met à jour les métadonnées contextuelles |
| `fill_section` | Remplit ou complète un nœud `LegalActContent` (article, considérant, visa…) |
| `georisques_query` | Interroge l'API GéoRisques pour les données d'une installation |
| `icpe_query` | Interroge les bases ICPE/AIOT pour les données administratives d'un établissement |
| `generate_numbering` | Recalcule la numérotation des nœuds après modification structurelle |
| `validate_structure` | Vérifie que l'acte respecte les invariants structurels avant génération |
| `delegate_to_expert` | Réservé au Superviseur : délègue une sous-tâche à un profil d'expert nommé du catalogue |
| `spawn_expert` | Sous-tâche dynamique : confie une sous-tâche à une nouvelle instance du Superviseur, qui choisit lui-même l'expert approprié (voir `agent::orchestration::AgentFrame::nested_supervisor`) — utile à un expert qui identifie, en cours de tâche, un besoin dont il ne sait pas lui-même à quel profil du catalogue le confier |

L'agent peut composer ces outils en séquence pour rédiger tout ou partie d'un arrêté, compléter les visas réglementaires, ou vérifier la conformité des seuils ICPE.