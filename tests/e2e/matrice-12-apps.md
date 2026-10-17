# Recette insertion — matrice de 12 applications

Source : docs/06 §7 (cible recette obligatoire avant release ; seul le
sous-ensemble ① — 6 lignes « P0-spike » — est exigé pour le go/no-go de la
spike, les 6 suivantes arrivent avant la release V1).

Protocole par ligne :
1. compiler et lancer Plume ; noter la version dans le tableau ;
2. positionner le focus dans le champ décrit ci-dessous ;
3. dicter la phrase de référence (« Deux mois de dictée, aucun souci. ») ;
4. relever : niveau utilisé (journal `InsertResult`), temps d'insertion
   (`Latency` de stage `insert`), verdict critique (citée ci-dessous).

| # | Application | Type de champ | Méthode attendue | Verdict (à relever) |
|---|---|---|---|---|
| 1 | Notepad (Windows 11) | Contrôle simple | ValuePattern (niveau 1) | Insertion correcte ; annulation par Ctrl+Z |
| 2 | Word (classique) | Édition riche | Presse-papiers (niveau 2) | Insertion, style conservé, presse-papiers restauré |
| 3 | Outlook (classique) | Édition riche | Presse-papiers, maintien 3 s (`slowApps`) | Insertion complète, aucun ancien contenu collé |
| 4 | Outlook (nouveau) | WebView | Presse-papiers | idem #3 |
| 5 | Teams | WebView | Frappe ou presse-papiers | Insertion, focus conservé |
| 6 | Chrome — Gmail | `contenteditable` | Presse-papiers | Insertion avec retours à la ligne corrects |
| 7 | Chrome — Google Docs | Surface propriétaire | Presse-papiers | Insertion ; limites documentées |
| 8 | Slack (desktop) | Electron | Frappe ou presse-papiers | Insertion sans doublon |
| 9 | VS Code | Electron, éditeur | Profil « verbatim », frappe | Symboles et indentation non reformatés |
| 10 | Windows Terminal | ConPTY | Presse-papiers (collage utilisateur) | Insertion ; comportement documenté |
| 11 | mstsc (Bureau à distance) | Fenêtre distante | Presse-papiers, maintien 5 s (`remoteDesktop`) | Insertion après restauration ; latence spécifique |
| 12 | Notion (desktop) | Electron | Presse-papiers | Insertion, aucun échec silencieux |
| ○ | Application élevée (admin) | Contrôle négatif | **Échec attendu** | Message neutre, texte récupérable via fenêtre de secours |

Critères de release (docs/06 §5) :
- **P0-spike** : lignes 1, 2 + une WebView (5 ou 6) + contrôle négatif ○, avec
  verdict documenté par application ;
- **V1 MVP** : 12 lignes ≥ 90 % de réussite, sur Windows 10 22H2, Windows 11
  23H2 et 24H2 (M1 au minimum) ; les lignes 3, 4, 10, 11 et ○ sont testées sur
  les 4 profils machines.

Relevé par session (copier ce bloc à chaque passe) :

```text
date / build / OS / profil :
| # | App | Niveau utilisé | latence insert (ms) | Verdict | note |
|---|-----|----------------|---------------------|---------|------|
|   |     |                |                     |         |      |
```

Journalist des annexes HTML (captures d'écran) obligatoire pour tout défaut constaté.
