# La vie privée dans Voxli — ce qui se passe avec vos données

> Document grand public. Il résume le comportement RÉEL du logiciel tel que
> codé, et renvoie aux documents techniques pour le détail (RF + ADR dans
> `docs/02`/`docs/03`). Formulation exacte testée : RF-35 (« 100 % local »
> encadré).

---

## 1. Le principe

Voxli écoute le micro SEULEMENT quand vous le demandez (raccourci de
dictée), transforme la parole en texte SUR CETTE MACHINE, et place le
résultat dans la fenêtre active. Aucun envoi réseau n'a lieu pendant ce
cycle. C'est une propriété vérifiable du code : l'application ne comporte
aucune télémétrie (`telemetry: off` par défaut, non activable même en
éditant la configuration — l'option est ignorée).

## 2. Votre audio

- N'est JAMAIS écrit sur le disque. Il vit en mémoire le temps de la prise
  (et, si une transcription échoue, le temps de la récupération — bouton
  « Réessayer », améliore le résultat sans repartir de zéro ; vous pouvez
  tout annuler).
- Ne quitte jamais la machine, sauf le SEUL cas ci-dessous (§5 : moteur
  distant, opt-in).

## 3. Vos textes (transcriptions)

- Par défaut : conservés EN MÉMOIRE seuls (disparaissent à l'extinction),
  liste consultable dans Réglages → Historique, supprimable intégralement.
- Optionnel (Pro, consentement requis) : historique au disque, chiffré
  par la clé système Windows (DPAPI) — il ne se lit QUE par ce compte
  Windows SUR cette machine.
- Le texte peut rester dans la fenêtre de secours si l'insertion a
  échoué : c'est le tampon interne de Voxli, sur votre machine.

## 4. Le presse-papiers

- Voxli n'y touche que pour DÉPOSER le texte dicté quand c'est le moyen
  d'insertion choisi, et il reprend presque aussitôt exactement ce que
  vous y aviez (délais par application, configurable).
- Copier une transcription requiert un geste explicite (bouton
  « Sélectionner tout » de la fenêtre de secours). Rien n'est lu au
  démarrage ni en tâche de fond.

## 5. Les SEULES touches réseau possibles — toutes explicites

| Cas | Quand | Qui décide |
|---|---|---|
| Téléchargement d'un modèle de reconnaissance | JAMAIS automatique : bouton « Télécharger » de l'écran Modèles | l'utilisateur |
| Transcription distante (moteur remote, BYOK) | seulement si mode local strict désactivé + consentement donné + clé API saisie | l'utilisateur, révocable à tout instant |
| Post-traitement distant | idem, interrupteur séparé (off par défaut) | l'utilisateur |

- En mode local strict (DÉFAUT), ni l'un ni l'autre : même si un moteur
  distant était compilé dans le binaire, aucun octet n'en part.
- Une mise à jour AUTO du logiciel n'existe pas (aucun serveur de
  publication n'est interrogé ; la mise à jour passe par vous).

## 6. Ce que Voxli lit sur votre écran

L'application lit, aux moments stricts suivants, l'application ACTIVE et
son TITRE pour : refuser les champs de mots de passe, appliquer vos règles
d'exclusion, choisir le profil correct. Ce titre/app n'est jamais stocké
dans les journaux. Une lecture plus large (la SÉLECTION de texte) existe
seulement si vous y consentez explicitement — un interrupteur séparé dans
Réglages → Vie privée (RF-37, « aucune lecture » par défaut).

## 7. Le journal local

- Le diagnostic enregistre des CODES de problème (par ex. « capture/code
  3 »), des durées, des versions — JAMAIS ni un mot dicté, ni un titre de
  fenêtre, ni du presse-papiers (RF-34).
- L'export de données ne contient RIEN de confidentiel de plus que ce que
  vous voyez à l'écran : configuration + dictionnaires, à relire avant
  tout envoi.

## 8. Votre vocabulaire (dictionnaire)

Le dictionnaire d'apprentissage (noms propres, acronymes, variantes
orales) reste dans le fichier de configuration de la machine ;
export/import sont possibles dans les deux sens, en un fichier CSV de
votre propriété.

## 9. La source de ce document

Ce document décrit le CODE au jour de sa rédaction. S'il diverge un jour
du produit, le dépôt (docs/02 : RF-33/34/35 ; docs/03 : ADR-07/08) fait
foi — et ce fichier doit être mis à jour en conséquence.
