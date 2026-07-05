## Agent IA

L'agent opère par boucle agentique (ReAct) et dispose des outils suivants :

| Outil | Description |
|---|---|
| `legifrance_search` | Recherche dans la base Légifrance (textes législatifs, jurisprudence) via API officielle (`/search`) |
| `legifrance_fetch` | Récupère le contenu complet d'un texte ou d'un article par identifiant, via la route `/consult/*` adaptée à son fonds (`legiPart`, `code`, `juri`, `jorf`, `getArticle`) |
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