# Fixtures audio — recette locale

Ces fichiers WAV (mono 16 kHz PCM16) servent à la recette manuelle et aux
futurs tests d'intégration du moteur réel. Ils sont **générés** par
`scripts/gen-fixtures.ps1` : rien de binaire n'est suivi dans le dépôt,
rechargez-les après un `git clean -xd`.

| Fichier | Durée | Contenu | Verdict attendu |
|---|---|---|---|
| `silence-2s.wav` | 2 s | silence pur | VAD → Silence ; aucune insertion (RF-18) |
| `voix-synthetique-3s.wav` | 3 s | somme de 220/440/880 Hz | VAD → Speech ; réel : sortie moteur non déterministe, motifs d'hallucination filtrés (RF-18) |
| `short-100ms.wav` | 100 ms | tonalité 440 Hz | VAD → TooShort (seuil 250 ms) ; aucune insertion |
| `clic-clavier-1s.wav` | 1 s | salve bruitée puis silence | VAD → Silence ; aucune insertion |
| `long-10s.wav` | 10 s | tonalité 300 Hz | mesure de latence (docs/06 §4), durée maximale non atteinte |

Vérification rapide après une session de dictée :

1. lancer Plume (fenêtre réglages via le tray) ;
2. vérifier dans le journal (`%APPDATA%\Plume\logs\plume.log`) que chaque
   session consignée en `SessionDone` porte l'étape `vad` et un verdict
   cohérent — le contenu n'apparaît jamais (RF-34).
