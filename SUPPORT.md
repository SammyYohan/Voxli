# Support

Ce document explique comment diagnostiquer Voxli **sans jamais transmettre
de contenu dicté**. La conception (RF-34, ADR-10) fait en sorte que le
journal ne contienne pas le texte des transcriptions ; lisez tout de même
le fichier avant de l'envoyer (Règle d'or §3).

## 1. Où sont les fichiers

| Source de données | Chemin |
|---|---|
| Journal (JSON, sans contenu) | `%APPDATA%\Voxli\logs\voxli.log` |
| Configuration | `%APPDATA%\Voxli\config.json` |
| Modèles téléchargés + empreintes | `%APPDATA%\Voxli\models\` + `manifests.json` |
| Historique (si activé, chiffré) | `%APPDATA%\Voxli\history.sqlite` |
| Licence | `%APPDATA%\Voxli\license.txt` |
| Secrets (clé d'API distante, opt-in P4) | DPAPI via `voxli-secure`, jamais en clair |

## 2. Ce qu'un rapport de diagnostic contient

Depuis Réglages → « Diagnostic & données » → **Exporter le rapport de
diagnostic** : le rapport copie le journal + l'uptime du processus dans un
fichier, à côté duquel il vous laisse choisir l'expédition. Il ne contient,
par construction :

- ni le texte d'aucune dictée,
- ni l'audio,
- ni le contenu du presse-papiers,
- ni rien de la configuration des raccourcis au-delà des combinaisons elles-mêmes.

**Lisez le fichier** (il s'ouvre dans un éditeur de texte) avant tout envoi ;
en cas de doute, il peut être édité : chaque ligne JSON est indépendante.

## 3. Comment lire le journal

Chaque ligne est un événement JSON :

```jsonc
{"ts": 1792230244509, "event": "error", "module": "hotkeys", "code": 4}
```

- `event: "error"` + `module` + `code` : voir la table des codes dans
  `apps/desktop/src-tauri/src/strings.rs` (`msg_error`).
- `event: "device_event", kind: "…"` : événements matériel, fenêtre,
  raccourcis (`register-pushToTalk-infructueux:…` = combinaison déjà prise
  par une autre application, ADR-05 — le produit continue sans elle).
- `event: "engine_loaded"` : moteur réellement retenu (`fake` en
  développement, `whisper.cpp` / `parakeet` / `nemo` avec la feature + le modèle).
- `event: "state_changed"` : machine à états (`idle` → `recording` → …).
- Aucune ligne ne contiendra le texte transcrit (garantie RF-34, testée).

## 4. Signaler un problème

Le canal de support n'est pas encore ouvert (phase de test interne). En
interne : joindre le rapport de diagnostic + une description des étapes avec
l'application au premier plan au moment du problème — **ne joignez jamais
l'audio ni une capture du texte dicté** sauf si vous le souhaitez
explicitement.

## 5. Réinitialisation complète

Pour restaurer les réglages d'usine sans désinstaller :
1. Quitter Voxli depuis le tray.
2. Supprimer ou renommer `%APPDATA%\Voxli\config.json`.
3. Relancer Voxli. Les modèles déjà téléchargés sont conservés.
