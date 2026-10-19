# 12 — Rapport d'audit UX/UI et intimité du produit

> Date : 2026-10-19 · Scope : main `990bea7` (M1.5 inclus) · Lecture seule, aucune modification de code.
>
> Posture : mis à la place de l'utilisateur final. Objectif : atteindre le niveau de fluidité de WisprFlow
> (dictée invisible, texte qui s'écrit sous les yeux, feedback permanent) **sans copier sa valeur ajoutée
> (cloud payant) mais en défendant celle de Voxli : 100 % local, privé, transparent.**

---

## Synthèse exécutive (Top 10, par ordre d'impact utilisateur)

| # | Constat | Cause | Gravité |
|---|---------|-------|---------|
| 1 | **La pastille n'affiche jamais de texte en direct** (curseur clignotant seul) | `worker.rs` émet `s.snapshot()` (vide pour whisper) au lieu du texte drainé | 🔴 bloquant |
| 2 | **La dictée « est mal écrite »** : insertion d'insécables qui reformate `12:00` → `12 : 00`, caractères invisibles collés, corrections floues à faux positifs | `post.rs` niveau 1 actif par défaut | 🔴 bloquant |
| 3 | **L'écran moteur/modèles disparaît** sitôt un modèle installé | repli « advanced » globalement appliqué à `#sec-modeles` | 🔴 bloquant |
| 4 | Vérification des modèles impossible à interpréter (SHA-256 jamais rempli, « colonne contrôle » toujours ✗) | `expected_sha256: None` ×4 | 🟠 majeur |
| 5 | Téléchargements non annulables, sans nom de modèle, sans taille/ETA quand le serveur n'envoie pas `content-length` | UI minima | 🟠 majeur |
| 6 | Le thème ne s'applique pas au démarrage (flash clair en mode sombre, « système » ignoré) | script inline bloqué par la CSP + `applyTheme()` jamais appelé au boot | 🟠 majeur |
| 7 | Contraste des boutons en thème sombre ≈ 2:1 (WCAG échoué) | texte blanc sur accent `#a2aaff` | 🟠 majeur |
| 8 | Le modèle français auto-recommandé casse le garde RF-16 (détection aléatoire sur énoncés < 1,5 s) | fallback vide si `language=""` | 🟠 majeur |
| 9 | Filtre « anti-hallucination » trop large (`contains`) : une dictée légitime contenant « sous-titres » ou « bye-bye » est **effacée** | `whisper.rs` | 🟠 majeur |
| 10 | Barre de progression à 0 % figée, colonne « contrôle » dépourvue de badge, jargon interne (« docs/11 §3 ») exposé | UX naïve | 🟡 moyen |

---

## 1. Zoom critique — La chaîne des partiels (l'essence de WisprFlow en local)

### 1.1 Flux théorique (documenté et correct)

```
capture (tap mono @ taux natif)          crates/voxli-audio/src/capture.rs:135-150
  → slot.feed (try_lock)                 apps/desktop/src-tauri/src/worker.rs:612-625
  → tick 50 ms → StreamCmd::Pump         worker.rs:881-889
  → thread stream-driver → drain()       worker.rs:356-510
  → emit("dictation://partial", {text: s.snapshot(), atMs})   worker.rs:428-434
  → pill.ts listen("dictation://partial")  apps/desktop/src/pill.ts:89-95
  → <span id="partial"> + classes .live / .has-partial       apps/desktop/pill.html:12
```

Le nom d'événement match des deux côtés, les capabilities laissent écouter, vite compile `pill.html`.
**La chaîne n'est pas cassée côté transport.**

### 1.2 Cassure réelle : le texte est calculé puis jeté

- `worker.rs:421-434` : après `s.drain()` → `Ok(updates)`, le vecteur `updates` ne sert qu'à
  incrémenter `updates_seen` (lignes 424-425). L'émission utilise `s.snapshot()` (ligne 431).
- `whisper_partial.rs:223-227` : `WhisperPartialStreamer::snapshot()` retourne **`String::new()` en dur**
  (« le snapshot n'invente pas une seconde source de vérité »). Le texte réel, produit à `drain()`
  (`StreamUpdate::Partial { text }`, ligne 214), n'est consommé nulle part en production.
- Conséquence : chaque partiel whisper arrive à la pastille avec `text = ""`.
- `pill.ts:89-95` pose `.live` **et** `body.has-partial` inconditionnellement ; `pill.css:136-138`
  masque alors le libellé « Enregistrement… » dès `has-partial` → l'utilisateur voit
  **un curseur ambre clignotant sans texte**. C'est exactement le retour terrain.

### 1.3 Garde-fous qui réduisent encore la fenêtre d'affichage (à revoir même après le fix)

| Garde | Valeur | Fichier:ligne | Effet ressenti |
|-------|--------|---------------|----------------|
| Audio neuf minimum | 1,2 s | `whisper_partial.rs:30, 188-190` | une phrase courte → aucun partiel |
| Intervalle min re-décodage | 1 500 ms | `whisper_partial.rs:32, 192-195` | cadence max 1 affichage/1,5 s (lent pour du live) |
| Fenêtre décodée | 8 s glissantes | `whisper_partial.rs:28` | le **début** de la dictée disparaît visuellement de la pill |
| Auto-off silencieux | décodage > 2 500 ms | `whisper_partial.rs:35, 206-210` | partiels s'arrêtent **sans message** sur machine lente |
| Tampon 60 s | troncature tête | `whisper_partial.rs:38, 176-180` | long énoncé = seules les 8 dernières secondes visibles |
| Filtre hallucination | `contains` pan-phrase | `whisper_partial.rs:84` + `whisper.rs:59-70` | mot « innocent » → partiel vide |

### 1.4 Autres moteurs : la promesse « cross-engine » n'est pas tenue

- **Parakeet** : `streaming: None` (`worker.rs:1164-1168`) → jamais de partiel.
- **FakeCapture** (repli sans engine réel) : pas de tap → zéro partiel (`worker.rs:634, 1579-1582`).
- **Nemo** : seul moteur qui « marche » (`snapshot()` = `committed + tail`, `nemo.rs:430-436`),
  mais n'est pas compilé dans le build nominal (`default = []` dans le Cargo.toml Tauri).

### 1.5 Ressenti visuel — pourquoi l'« animation d'entrée » semble perdue

- L'apparition du texte n'a **aucune animation** (pas de `transition`/`animation` sur `.partial.live`,
  `pill.css:142-166`) : le texte poppe sec, s'il arrive (il n'arrive pas, cf. 1.2).
- La ligne est `white-space: nowrap; overflow: hidden; text-overflow: ellipsis` sur une seule ligne
  (~560 px utiles, `pill.css:146-153`) : pas de scroll, pas d'historique visuel, textes longs illisibles.
- `pill.css:136-138` masque le libellé à l'aide de `has-partial` — classe posée même si le texte est vide
  (le mauvais signal supplante le bon).
- Effacement immédiat à tout état ≠ recording (`pill.ts:113-130`) : la transition d'entrée est cassée
  par un effacement brutal, pas un fondu.

**Correctifs cibles (M2-UX)** :
1. `worker.rs:431` : émettre le texte des `updates` drainés (`StreamUpdate::Partial { text }`) —
   ou faire renvoyer le vrai texte par `snapshot()` ; retirer la pose de `.has-partial` si `text` vide.
2. Fenêtre partielle 8 s → macro configurable, et **rincer l'écran seulement quand la fenêtre glisse**,
   pas à chaque re-décodage.
3. Fade-in CUBE-scale sur `.partial.live` (150 ms ease-out), fondu de sortie à `state != recording`.
4. Auto-off silencieux → un message (`notice://notice`) « partiels désactivés (machine lente) » et non
   un arrêt furtif.

---

## 2. Zoom critique — « La dictée est mal écrite »

### 2.1 Le réglage par défaut mutile le texte dicté

`PostProcessing::Rules` est actif par défaut (`voxli-config/src/lib.rs:45-55`) et
`apply_french_spacing` (`crates/voxli-text/src/post.rs:312-346`) fait des dégâts :

- **Chiffres/temps/URLs reformés** : `12:00` → `12 : 00`, `3:2` → `3 : 2`. La garde « colon_url »
  ne couvre que `://` (URL en plein milieu). Un horaire dicté devient visuellement faux.
- **Caractères invisibles** : U+00A0 inséré avant `: ! ? ; % €` (lignes 317-343). Collé dans un
  éditeur de code, un terminal, un champ HTML, la rapidité de saisie semble « morte ».
- **Correction floue à faux positifs** (`post.rs:450-502`) : remplacement insensible casse/accents
  puis Levenshtein ≤ 1 sur tout mot ≥ 5 lettres, pour **toutes** les entrées du dictionnaire, sur
  tout le texte. Un mot légitime voisin d'une entrée métier est silencieusement réécrit.
- Le dictionnaire s'applique **aussi en verbatim** (`post.rs:37-49`) : un utilisateur en mode brut
  voit quand même son texte corrigé (incohérence d'intention).

### 2.2 Whisper : réglages sous-optimaux pour le FR (whisper.rs)

| Paramètre | État | Ligne | Impact « mal écrit » |
|-----------|------|-------|---------------------|
| `temperature = 0.0` **sans fallback** | problème | 114 | segments ratés restent ratés (aucun retry planifié) |
| `Greedy { best_of: 1 }` | problème | 100 | pas de beam, robustesse dégradée |
| `condition_on_previous_text = false` | choix assumé | 117 | anti-boucle ✓, mais amnésie de contexte → ponctuation/majuscule incohérentes inter-segments |
| `suppress_tokens` non réglé | manque | absent | reste sur le défaut cpp |
| Threads `min(4, cores)` **non réglable** | RF-17 partiel | 106-112 | le réglage utilisateur n'arrive jamais (config sans champ `threads`) |
| Filtre hallucination `contains` sur-étendu | **bug qualité** | 59-70 | « goodbye », « sous-titres », « amish » … → `None` → **texte entier effacé**, `NothingToTranscribe` |
| Segmentation 116 tokens non gérée | manque | n/a | longs énoncés non chunkés |
| `initial_prompt` = dictionnaire ≤ 512 car. | bien | whispered.rs:118-122 | mais `WhisperPartialSession` passe `initial_prompt: None` (`whisper_partial.rs:63-68`) : les partiels ne voient JAMAIS le vocabulaire métier → incohérence affichée vs produit final |

### 2.3 Auto-détection de langue : l'option « recommandée » de l'UI dégrade la qualité

- `index.html:94-96` : « Détection automatique (recommandée) » avec `value=""`.
- `worker.rs:1044` → `language = None` ✓ auto ; **mais** `worker.rs:1047` →
  `auto_language_fallback = None` aussi : le garde RF-16 (`orchestrator.rs:555-558`) ne retombe
  plus sur rien. Un énoncé < 1,5 s → whisper auto-détecte sur 2 mots → langue aléatoire
  (invasions d'anglicismes, passages FR → EN non voulus).
- En même temps, `language` **par défaut est `"fr"` en dur** (`voxli-config:145`), l'utilisateur
  n'a pas conscience que « auto » est risqué pour les phrases courtes.

**Correctif cible** : en mode auto, le fallback RF-16 doit retomber sur la langue UI (`ui_language`)
ou sur un vote glissant des N dernières détections, jamais sur `None`.

### 2.4 Textes UI mal orthographiés / mal formés (à corriger)

- `index.html:179` : « **soundanais** (su) » → *soudanais*.
- `index.html:158` : « nynorsk **norvégien** » → « norvégien (nynorsk) ».
- `i18n.ts:265-268` : « rien n'est conservé (défaut **navigateur**) » — absurde pour une app
  desktop (voulu : « par défaut »).
- `index.html:97-195` : ~70 options de langue **sans `data-i18n`** → restent FR en UI anglaise.
- `rescue.html:25` : bouton affiché « **Sélectionner tout** » mais exécute une **copie**
  (`rescue.ts:38-45`, via `document.execCommand("copy")` déprécié, en mutant temporairement le
  champ `readonly`) → libellé trompeur + usage d'API dépréciée.
- Ordre des boutons de secours (`rescue.html:23-26`) : « Réessayer » (destructif : consomme l'audio)
  en premier, « Copie » (geste le plus sûr) en troisième — hiérarchie d'urgence inversée.

---

## 3. Zoom critique — « Je ne vois pas ce que je télécharge »

### 3.1 L'écran semble fuir l'utilisateur

- `main.ts:362-367` : dès qu'UN modèle est installé, `#sec-modeles` est repositionné en `advanced`.
  Or les radios de moteur **et** la table de téléchargement vivent tous les deux dans cette section
  (`index.html:301-328`). En mode Simple (défaut), **tout l'écran moteur/modèle s'efface** juste après
  le téléchargement d'onboarding → le résultat réel paraitre invisible. Modification requise :
  ne replier **que** la table des modèles, garder toujours visibles les radios `#engines-box`.

### 3.2 Téléchargement : ce qui manque pour « voir »

1. **Aucun annulateur branché** : la commande `models_download_cancel` existe
   (`commands.rs:556-559`, enregistrée `main.rs:138`) mais **aucun bouton** y fait référence
   (grep vide côté `src/`). Un téléchargement de 57-708 Mo ne peut être annulé qu'en fermant la fenêtre.
2. **Une seule barre globale sans nom** : `#model-progress` (`index.html:324-327`) affiche `x/y Mo`
   mais jamais *quel modèle*, jamais % textuel, jamais ETA.
3. **Progression à zéro figée** si le serveur n'envoie pas `content-length` (`total=0`,
   `main.ts:790-802`) : pas de fallback « octets reçus, taille inconnue » élégant.
4. **Vérification impossible à comprendre** : `expected_sha256: None` ×4 (`commands.rs:358, 366,
   374, 382`) → `verified` reste `false` (`commands.rs:427-431`, `856-887`). L'utilisateur voit
   « vérification en attente » ad vitam. Sans digest signé, **aucune authenticité du modèle
   téléchargé n'est prouvée** — combiné aux URLs HF codées en dur, c'est un trou de chaîne de
   confiance.
5. **Le clé actuel nominal compile sans moteur** (`Cargo.toml:36-37` `default = []`) :
   l'utilisateur peut télécharger 708 Mo de NeMo qui ne pourront **jamais** dicter dans ce build
   (`usable=false` → radios grisées « SDK non inclus »). Aucun message préventif avant le
   téléchargement, aucun bouton « Pourquoi ? ».
6. **Onboarding hardcodé** : le bouton 1-clic vise `"ggml-base-q5_1.bin"` (`main.ts:730-741`),
   taille « 57 Mo » répétée en dur dans l'i18n (`i18n.ts:48-55, 404-407`).
7. **Catalogue 100 % en dur** : `MODEL_CATALOG` 4 entrées (`commands.rs:351-384`), URLs en dur
   (`commands.rs:641-651`), noms de répertoires codés couche par couche (`worker.rs:1077, 1081-1083`,
   `worker.rs:1246-1250`).

**Correctifs cibles** :
- Brilliance de la pastille de téléchargement : carte par modèle (nom, taille connue/inconnue,
  %, ETA, vitesse, bouton Annuler, badge SHA-256 ✓/✗/—).
- Brancher `models_download_cancel` sur un vrai bouton Annuler + état « occupé » désactivant
  le second clic.
- Publier les SHA-256 officiels dans la config du catalogue **et les signer** (clé de release
  Ed25519 : cohérence avec la licence).
- Message préventif quand le build n'inclut pas le moteur avant le lancement d'un téléchargement.
- Écran « modèle installé » en Simple : une ligne « whisper-base · installé · 57 Mo · ✓ » et le
  lien « Modifier / Télécharger d'autres voix » (= Mode avancé).

---

## 4. Zoom critique — Hardcoding (ce qui « ne devrait pas l'être »)

### 4.1 UI / fenêtres / pastille

| Élément | Valeur | Fichier:ligne |
|---------|--------|---------------|
| Pill width/height/marges/hystérésis | 640/74, top 8, hover 15/20 px | `main.rs:209-215`, `main.rs:179-189` |
| Pill resricted au moniteur principal | x = écran0 | `main.rs:222-238` — dictée sur 2e écran = aucun retour là où on tape |
| Dot center manuellement couplé au CSS | `pos.x + 19.0*scale` etc. | `main.rs:318-322` ↔ `pill.css:106` |
| Cadences de maintenance | poll 120 ms, repin 0,5 s, place 2,4 s | `main.rs:270-277` màj en continu même à l'idle |
| Rescue window 540×440 / min 380×320 | | `worker.rs:1531-1537` |
| Réglages window 780×620, min 620×480 | titre FR en dur | `tauri.conf.json:15-24` |
| Pill fonts/palette | Segoe UI 13/14 px, #374151, #6b7280… | `pill.css:19-22, 42, 67-90, 104-127, 146, 176-182, 189-191` |
| Historique UI 50 lignes max | `n:50` | `main.ts:253` (backend clamp `commands.rs:1154`) |

### 4.2 Moteurs / chemins

| Élément | Valeur | Fichier:ligne |
|---------|--------|---------------|
| Catalogue 4 modèles + 4 × SHA None | liste figée | `commands.rs:351-384` |
| URLs de téléchargement | HF ×2 + GitHub releases ×1 | `commands.rs:641-651` + doublon `scripts/fetch-model.ps1:13` |
| Noms de fichiers modèles | écho code/UI/tests ×3 | `worker.rs:1077, 1081-1083, 1060` ∥ `commands.rs:488, 499` ∥ `nemo.rs:465`, `worker.rs:1249` |
| Modèle fallback whisper | `"ggml-base-q5_1.bin"` | `worker.rs:1057-1065` + `main.ts:731-737` |
| Chemins données | `%APPDATA%\Voxli` / `./voxli-data` | `voxli-config/lib.rs:440-445` ; **docs/05 §5 dit `%LOCALAPPDATA%`** → divergence doc/code |
| Bureau couplé `USERPROFILE\Desktop` | export data / diagnostics | `commands.rs:1261-1262, 1360-1362` — casse avec profil OneDrive redirigé |
| Logs | `logs/voxli.log` | `main.rs:43`, `diagnostics/lib.rs:74-77` |

### 4.3 Seuils temporels / audio (visibles sans réglage)

| Seuil | Valeur | Fichier:ligne |
|-------|--------|---------------|
| Tick worker | 50 ms | `worker.rs:62` |
| Fermeture streams | 600 / 2 500 ms | `worker.rs:170, 537` |
| Pré-roll max config | 100..1000 ms (défaut 400) | `voxli-config:208, 402-406` |
| Max recording | 30..1800 s (défaut 300) | `voxli-config:209, 403` |
| VAD min_speech_ms | 250 ms | `voxli-config:228` |
| Seuils VAD agressivité | 0.010 / 0.005 / 0.002 | `worker.rs:1593-1597` |
| Clipboard hold | 500 / 3000 / 5000 ms | `voxli-config:277-285` |
| Retries clipboard open | 10 × 20 ms | `voxli-win/src/clipboard.rs:64-74` |
| Tampon partiel | 60 s | `whisper_partial.rs:38` |
| Grace licence | 30 j fixes | `commands.rs:107, 177` |
| Ring pré-roll | 2,0 s | `capture.rs:22` |
| Chunk cap batch | 1800 s | `capture.rs:249-253` |
| Téléchargement connect/read | 20 s / 30 s | `commands.rs:684-687` |
| Retry auto off partiel | décodage > 2500 ms | `whisper_partial.rs:35` |
| Historique mémoire | 200 entrées | `worker.rs:1676-1677` |

### 4.4 Langues / textes FR en dur hors i18n

- **`strings.rs` entier (102 lignes)** : MSG_* ERR_* en français **const**. La notification
  (`notice://notice`) reste FR même quand `ui_language = "en"` (`worker.rs:1346-1513`) — incohérence
  bilingue.
- `worker.rs` « Aucune clé publique… » / « licence écrite… » (`commands.rs:174, 226-246`) erreurs fr.
- `capture.rs:449` « microphone sans nom ».
- `commands.rs` toutes les erreurs format! (fr exclusivement).

---

## 5. Global UX — Autres constats (numérotés)

1. **Thème au boot** : `index.html:8-21` script inline définissant le thème avant peinture est
   **bloqué par la CSP** (`tauri.conf.json:27` `default-src 'self'`), et `applyTheme()` n'est jamais
   appelée dans `main()` (`main.ts:744-763`). Démarrage = flash clair garanti en sombre,
   préférence « système » ignorée.
2. **Contraste boutons en sombre** : texte `#fff` sur `--accent: #a2aaff` (`style.css:166-173,
   319-331`, `.btn-primary:412-425`) → ratio ≈ 2:1, WCAG AA échoué sur **tous** les boutons primaires.
3. **Modèle de sauvegarde incohérent** : moteur, dictionnaire = **immédiats** ; hotkeys/audio/privacy =
   **bouton « Enregistrer les réglages »** dans la section Audio (`index.html:296`) alors que
   l'index promet « Raccourcis appliqués immédiatement » (`main.ts:103-106`) → désorientation.
4. **Événement notice sans ARIA** : `#notice` sans `role="status"`/`aria-live`, auto-masqué en 6 s
   (`index.html:34`, `main.ts:115-123`) → erreur disparue avant lecture.
5. **Vumètre sans libellé ARIA**, badge d'état inoccupé (`index.html:71-80`).
6. **Rescue sans clavier** : pas de Enter sur Réessayer, pas d'Esc sur Fermer.
7. **KeyNames incomplets dans la légende de pill** : `keyName()` translittère chaque jeton via
   `t(key.X)`, mais seules 11 clés sont définies (`pill.ts:15-22`, `i18n.ts:437-446`) → un
   raccourci `Ctrl+Alt+V` affiche « key.V » brut.
8. **Raccourci par défaut `Ctrl+Alt+Space` en conflit potentiel IME/switch layout** ; le mode
   `use_low_level_hook` existe mais n'est pas expliqué dans l'UI (`voxli-config:190`).
9. **Notice d'erreur « Moteur en cours d'exécution » masque la ligne “moteur effectif” en silence**
   (`main.ts:429-440`) si `state_get` échoue.
10. **`emit` global vs fenêtre ciblée** : `state://changed`, `meter://level` diffusés à toutes les
    fenêtres (`worker.rs:1329-1339`) alors que `pill://hover` est correctement ciblé (`main.rs:283`)
    — composable, mais formation des idiomes à choisir.
11. **Diagnostic DeviceEvent.kind : String libre** dans `DeviceEvent` connu (`diagnostics/lib.rs:
    293-341`) : un pilote peut glisser du texte d'app dans le rapport de support — contenu non dicté,
    mais vecteur d'infos.
12. **`dictation://partial` n'atteint pas la fenêtre main** (main.ts n'écoute pas) : correct au sens
    RF-34, mais pendant une prise dans la fenêtre rescousse (readonly), l'utilisateur n'a **aucun
    écho local**. Ajouter une ligne live ou un indicateur « en écoute… ».
13. Titre FR en dur dans `tauri.conf.json` (“Voxli — Réglages”), même en UI anglaise.

---

## 6. WisprFlow, mais mieux (proposition → valeur ajoutée locale)

> WisprFlow gagne sur trois axes : le texte arrive **instantanément sous les yeux** partout,
> le produit est **oubliable** (aucun réglage à découvrir), la **reformulation IA** tolère le
> bafouillage. Voxli peut viser le même niveau d'UX **en restant 100 % local**, et y ajouter
> sa propre valeur : la confiance vérifiable.

### 6.1 « Partout sous les yeux » (à converger avant tout)

La pill existe mais ne montre rien (§1) et est **épinglée au moniteur principal** seulement
(`main.rs:222-238`). Actions :
- Fixer la chaîne partielle (1.2) ;
- Pill multi-moniteurs : position sur le moniteur du **foreground** (GetWindowRect sur hwnd focus),
  déjà disponible via `voxli-win::foreground`.
- Bande live plus lisible : garder le texte **fin d'événements uniquement** (pas de re-render
  sur doublons), monter jusqu'à 2 lignes et laisser continuer dans `rescue.ts` readonly.
- Ajouter fade-in et curseur word-scroller : le fade de la classe `.partial.live` (150 ms).

### 6.2 La fluidité de dictée, sans cloud

WisprFlow sur-sélection des mots ; on peut converger comportementalement **sans cloud** :
- **Fenêtre partielle P1** : écho live avec **re-render différentiel** (pas de reflash) ;
- **Anti-hallucination ciblée** : remplacer `contains` par un match **en début de segment**,
  et une logique de crédit (score logit noyaux requis, seilement pour tokens de début) ;
- **Fallback température** : retry à T=0.2 / 0.4 sur segments vides ou trop courts,
  comme le mécanisme de retry natif d'OpenAI Whisper — coût zéro pour le local (juste du CPU en fin de segment).
- **Dictionnaire prédéfini par contexte** (`Code.exe` etc.) branché aussi aux partiels
  (`initial_prompt` dans whisper_partial.rs) pour que texte live = texte final.
- **Typographie FR maîtrisée** : l'espace insécable est une règle **française correcte** —
  `apply_french_spacing` doit la conserver pour la ponctuation "propre" (`mot  :` est à éviter),
  mais **jamais** reformer le contenu dicté (horaires, ratios, URLs). Un mode
  « Texte brut (verbatim) » court-circuitant tout le post-traitement, exposé en UI, mémorisé
  du dernier choix par app.
- **Editable mic gain** : seuils VAD 0.010/0.005/0.002 en dur (`worker.rs:1593-1597`) → régler
  par-device, sinon un micro doux disparaît (faux « Aucune voix exploitable »).

### 6.3 La confiance (la valeur ajoutée de Voxli à revendiquer HORS UI)

WisprFlow est opaque sur sa donnée. Voxli est, par conception, **l'inverse** : afficher la
confiance plutôt que la cacher.
- Une carte « Mode de confidentialité » avec les faits réels (zéro réseau ASR, dictionnaire local,
  fichiers modèles vérifiés), en face des options — pas noyé dans la doc ;
- Méthode affichée là où on agit : à droite de chaque téléchargement, le SHA-256 **vérifié** (badge ✓
  par couleur, pas jargon « docs/11 §3 ») ;
- Un jour, une chaîne de confiance publique (manifest signé) rend la promesse **auditable par un
  tiers** — ce que WisprFlow ne peut pas se permettre.

### 6.4 Ce qu'il faut AJOUTER que WisprFlow n'a pas

| Idée | Local & différenciant |
|------|----------------------|
| **Historique réutilisable** | Aujourd'hui l'historique garde 200 entrées en mémoire (`worker.rs:1676-1677`) et 50 en UI (`main.ts:253`) sans recherche ni export — ajouter recherche, export MD/JSON, réinsertion à chaud |
| **Réglage précis des partiels** | Exposer « vitesse d'affichage live » (1,5 s → 0,5 s) et fenêtre ›8 s ; le noyau a déjà tout, juste pas branlé |
| **Annulation d'insertion (Undo)** | Envoi natif de Ctrl+Z après l'insertion (SendInput, ou alias clipboard) — permet de revenir sur une dictée ratée sans perdre le focus |
| **Validation par notification** | `models://done` poste pour une fenêtre Visible si la fenêtre réglages est minimisée (state timeout) |
| **Onboarding mieux pensé** | Le choix du modèle par défaut est figé → l'onboarding 1-clic pourrait afficher quel moteur sera utilisé, et proposer `small` (181 Mo, meilleure qualité FR) en option explicite avec le temps de téléchargement estimé |

---

## 7. Priorisation — Ce qu'il faut corriger pour que le produit « se débloque »

### P0 — Cette semaine (l'essence est là mais muette)

| # | Tâche | Fichier / lieu |
|---|-------|----------------|
| 1 | Émettre le texte réel des `updates` drainés et non `snapshot()` ; ne pas poser `.has-partial` si `text` vide | `worker.rs:424-435`, `pill.ts:89-95` |
| 2 | Brancher le bouton Annuler sur `models_download_cancel` + état occupé | `main.ts` |
| 3 | Ne replier que la table des modèles ; garder `#engines-box` toujours visible en Simple | `main.ts:362-367` |
| 4 | Fenêtre 8 s → config + mnemonic ; fix mémoire du post (« suppression d'insécantes » pour `12:00` ; garder l'insécable typographique partout ailleurs) | `whisper_partial.rs:28-38`, `post.rs:312-346` |
| 5 | Fix faux-positifs du filtre anti-hallucination (`contains` → début de segment) | `whisper.rs:59-70` |
| 6 | Fallback RF-16 auto-langue : retomber sur `ui_language`, jamais `None` | `worker.rs:1044-1047`, `orchestrator.rs:555-558` |
| 7 | Thème appliqué au boot (`applyTheme()` dans `main()`), PAS de script inline bloqué par CSP | `main.ts:744`, `index.html:8-21` |
| 8 | Contraste boutons sombre (accent plus foncé, ex. #5b6bd6) | `style.css:316-331` |

### P1 — Les 15 jours suivants (friction et confiance)

9. Carte de téléchargement par modèle (nom, %, ETA, SHA-256 réel, Annuler, badge vérifié).
10. Digests + manifest signé (publiquement auditable) pour le catalogue.
11. Pill multi-moniteurs + fade-in du texte + 2 lignes scrollables.
12. Unifier les sauvegardes : tout **immédiat** ou tout au bouton, jamais mélangé.
13. Langue auto fiable (vote glissant) + `initial_prompt` sur les partiels (cohérence live/final).
14. i18n des notices noyau (`strings.rs`), des 70 langues carousel, du titre de fenêtre.
15. ARIA : `role="status"` sur `#notice`, labels du vumètre, raccourcis clavier Esc/Enter dans la fenêtre de secours.

### P2 — Différenciation (au-delà de WisprFlow)

16. Écran modèle « Simple » : ligne « whisper base · installé · ✓ » + bouton « Autre voix ».
17. Historique : recherche, export, re-envoi à chaud.
18. Réglage `threads` utilisateur (RF-17 complet), réglage VAD par microphone.
19. Écran d'état.confiance (réalité réseau “aucune”， emplacements fichiers).
20. Pagination de docs/05 où le chemin `%LOCALAPPDATA%` diverge du code `%APPDATA%`.

---

## 8. Annexes (détails du code vérifié ligne à ligne)

### A. L'émission partiel cassée, au mot près (vérifiée le 2026-10-19)

```rust
// apps/desktop/src-tauri/src/worker.rs — drain():
match s.drain() {
    Ok(updates) => {
        if !updates.is_empty() {
            slot.updates_seen.fetch_add(updates.len() as u64, Ordering::Relaxed);
            let _ = app.emit(
                "dictation://partial",
                PartialPayload {
                    text: s.snapshot(),   // <-- "" pour WhisperPartialStreamer
                    at_ms: now_ms(),
                },
            );
        }
    }
```

```rust
// crates/voxli-asr/src/whisper_partial.rs — snapshot:
fn snapshot(&self) -> String {
    // Les partiels transitent par les événements `drain` ; le snapshot
    // (hors flux) n'invente pas une seconde source de vérité.
    String::new()
}
```

Les deux lignes se contredisent : le commentaire de whisper_partial.rs pose l'hypothèse que
`drain()` est lu par le consommateur — or le consommateur (worker.rs) ignore précisément
`updates` et relit `snapshot()`.

### B. Le trigger de l'incohérence .has-partial (vérifié)

```ts
// apps/desktop/src/pill.ts:89-95
void listen<PartialPayload>("dictation://partial", (ev) => {
    const el = document.getElementById("partial");
    if (el === null) return;
    el.textContent = ev.payload.text;
    el.classList.add("live");
    document.body.classList.add("has-partial");
});
```

```css
/* pill.css:136-138 — libellé caché quoi qu'il arrive, alors que le texte vide */
body[data-state="recording"].has-partial .label {
  display: none;
}
```

### C. L'écran moteur caché quand le moteur est là (vérifié)

```ts
// apps/desktop/src/main.ts:362-367 — littéralement : "dès qu'UNE voix dicte,
// le catalogue technique n'a plus besoin d'être vu"
document.getElementById("sec-modeles")
    ?.classList.toggle("advanced", models.some((m) => m.installed));
```

### D. Le spacing français qui reforme la dictée (vérifié)

```rust
// crates/voxli-text/src/post.rs:316-333 — le "trailing" et le "prev_alnum"
// réécrivent 12:00 → « 12 : 00 » et injectent U+00A0 avant : ! ? ; % €
```

Parfaitement légitime **typographiquement** pour un rendu document (traitement de texte, RTF),
désastreux dans un champ de code, un terminal, une formule ou un champ HTML. Le mode verbatim ne
désactive pas le dictionnaire (`post.rs:37-49`), et `PostProcessing::off` existe mais n'est jamais
sélectionné par défaut, ni présenté comme « Texte brut » dans l'UI.

---

*Fin du rapport — prêt à servir de backlog pour M2-UX.*