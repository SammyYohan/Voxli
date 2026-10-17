# Livrable 4 — Diagrammes et flux

Les diagrammes sont en syntaxe Mermaid (rendus par GitHub, VS Code et la plupart des visionneuses
Markdown). Ils décrivent le comportement **cible** ; l'écart avec le prototype P0 sera documenté
dans le journal de mesures.

---

## 1. Vue des composants et frontières de confiance

```mermaid
flowchart TB
    subgraph Machine["Poste de travail Windows (frontière de confiance)"]
        subgraph Proc["Processus Plume.exe (non élevé)"]
            UI["Interface Tauri / WebView2<br/>(réglages, historique, onboarding)"]
            Core["Noyau Rust — chemin critique"]
            Audio["Capture WASAPI<br/>(cpal, thread temps réel)"]
            Pipe["Pipeline<br/>VAD → assemblage → moteur → post-traitement"]
            Inject["Insertion<br/>UIA → presse-papiers → SendInput → secours"]
            Store["Stockage<br/>config JSON · secrets DPAPI<br/>historique chiffré"]
        end
        Win["API Windows<br/>UIA · presse-papiers · SendInput<br/>RegisterHotKey · Credential Manager"]
        Apps["Applications cibles<br/>Word, Outlook, Teams, Chrome,<br/>VS Code, Terminal, RDP…"]
    end

    subgraph Distant["Zones distantes (jamais utilisées sans consentement)"]
        MCloud["Service de nettoyage cloud<br/>(facultatif, opt-in)"]
        MUpdate["Serveur de mise à jour<br/>(artefacts signés)"]
        MModels["Source des modèles<br/>(téléchargement initial)"]
    end

    UI <-->|"IPC : états et commandes<br/>(jamais d'audio ni de texte)"| Core
    Core --> Audio
    Audio --> Pipe
    Pipe --> Inject
    Core --> Store
    Inject --> Win
    Win --> Apps
    Pipe -.->|"si et seulement si consentement explicite"| MCloud
    Core -.->|"vérification signée, désactivable"| MUpdate
    Core -.->|"téléchargement vérifié par empreinte"| MModels

    classDef dingue fill:#ffe6e6,stroke:#c00,stroke-width:1px
    class MCloud,MUpdate,MModels dingue
```

**Lecture** : le seul chemin traversant la frontière de confiance est en pointillés, à droite, et
chaque flèche correspond à un flux nommé, documenté et désactivable (RF-35).

---

## 2. Flux nominal : dictée locale complète

```mermaid
sequenceDiagram
    autonumber
    actor U as Utilisateur
    participant HK as Gestionnaire de raccourcis
    participant CAP as Capture audio (WASAPI)
    participant PIPE as Pipeline
    participant ASR as Moteur local (whisper.cpp / Parakeet)
    participant PT as Post-traitement local
    participant INS as Insertion
    participant WIN as Application cible

    U->>HK: Maintient Ctrl+Alt+Espace
    HK->>CAP: Démarrage (+ pré-roll de 400 ms)
    CAP-->>U: Pastille « enregistrement » + niveau
    U->>HK: Relâche la touche
    HK->>CAP: Arrêt (latence de détection ≤ 50 ms)
    CAP->>PIPE: PCM mono 16 kHz float32
    PIPE->>PIPE: VAD : voix détectée ? durée suffisante ?
    alt pas de voix exploitable
        PIPE-->>U: État « rien à transcrire » (aucune insertion)
    else voix exploitable
        PIPE->>ASR: Transcription (thread priorité réduite)
        ASR-->>PIPE: Texte brut + confiance
        PIPE->>PT: Règles locales (niveau 0/1)
        PT-->>PIPE: Texte final
        PIPE->>INS: Cible mémorisée à l'appui
        INS->>WIN: Insertion (cascade)
        INS-->>U: Pastille « inséré » (ou chemin d'échec)
    end
```

**Budget de latence cible (à mesurer, pas à promettre)** — pour un énoncé de 10 s :

| Étape | Cible p50 | Cible p95 | Comment c'est mesuré |
|---|---|---|---|
| Détection du relâchement | ≤ 60 ms | ≤ 120 ms | Horodatage interne (scrutation 50 ms) |
| Finalisation audio + VAD | 150 ms | 400 ms | Journal de pipeline |
| Inférence (moteur par défaut, CPU de milieu de gamme) | ≤ 800 ms | ≤ 1 800 ms | Mesure moteur (voir `docs/06`) |
| Post-traitement local (niveau 1) | ≤ 15 ms | ≤ 40 ms | Banc de test règles |
| Insertion (niveau 1 ou 2) | 60 ms | 500 ms | Journal d'insertion |
| **Total perçu** | **≤ 1,1 s** | **≤ 2,9 s** | Somme mesurée, machine de référence |

---

## 3. Machine à états de la session de dictée

```mermaid
stateDiagram-v2
    [*] --> Inactif
    Inactif --> Enregistrement : appui (ou appui bascule)
    Enregistrement --> Enregistrement : changements de périphérique<br/>(poursuite, avertissement)
    Enregistrement --> Finalisation : relâchement / durée max atteinte / annulation
    Enregistrement --> Inactif : annulation explicite (audio jeté)
    Finalisation --> Analyse : VAD
    Analyse --> Inactif : silence ou trop court
    Analyse --> Transcription : voix exploitable
    Transcription --> PostTraitement : succès
    Transcription --> ErreurRecuperable : erreur moteur
    ErreurRecuperable --> Transcription : « réessayer »
    ErreurRecuperable --> Inactif : abandon (audio conservé N minutes)
    PostTraitement --> Insertion
    Insertion --> Insere : succès (niveau 1-3)
    Insertion --> Secours : échec des niveaux 1-3
    Secours --> Inactif : texte récupéré par l'utilisateur
    Insere --> Inactif : retour à l'état initial
    Transcription --> Transcription : file d'attente de 1<br/>(appui pendant transcription)
```

**Invariants de la machine à états** (vérifiés par tests) :
1. Aucune transition ne peut perdre le texte transcrit (RF-20, RF-22).
2. Une seule session active à la fois ; une seconde demande est mise en file ou refusée explicitement
   (RF-05).
3. L'audio est libéré après insertion ou après expiration du délai de récupération.

---

## 4. Décision d'insertion (cascade)

```mermaid
flowchart TD
    A[Texte final prêt] --> B{Cible mémorisée<br/>toujours valide ?}
    B -- non --> Z[Fenêtre de secours]
    B -- oui --> C{Champ sensible ?<br/>IsPassword / app exclue / UAC}
    C -- oui --> Y[Insertion refusée + message neutre]
    C -- non --> D{ValuePattern<br/>disponible et éditable ?}
    D -- oui --> E[SetValue puis vérification de lecture]
    D -- non --> F{Politique du profil<br/>applicatif}
    F -- presse-papiers --> G[Sauvegarde du presse-papiers<br/>+ marquage anti-historique]
    G --> H[Coller Ctrl+V]
    H --> I{Contenu inséré ?}
    I -- oui --> J[Restauration différée du presse-papiers]
    I -- non --> K[SendInput Unicode]
    F -- frappe directe --> K
    K --> L{Texte présent ?}
    L -- oui --> M[Fin]
    L -- non --> Z[Fenêtre de secours + « coller à nouveau »]
    E --> M
    J --> M
```

**Règle de conception** : chaque niveau indique au suivant *pourquoi* il a échoué, et ce motif est
journalisé localement (sans contenu) afin de mesurer le taux de repli par application (hypothèse H3).

---

## 5. Flux distants : consentement et journalisation

```mermaid
sequenceDiagram
    autonumber
    participant U as Utilisateur
    participant UI as Interface
    participant CG as Garde de consentement
    participant CLOUD as Service distant
    participant J as Journal des transmissions

    U->>UI: Active « Nettoyage cloud » (page Confidentialité)
    UI->>U: Affiche : ce qui part (texte), vers qui,<br/>combien de temps, finalité
    U->>CG: Consentement explicite (action distincte)
    Note over CG: Consentement stocké localement,<br/>révocable à tout moment
    U->>UI: Diction d'un paragraphe
    CG->>CG: Fonction active ET consentement présent ?
    alt consentement absent ou révoqué
        CG-->>U: Refus + indication « mode local utilisé à la place »
    else consentement présent
        CG->>CLOUD: Envoi du texte (jamais l'audio, jamais le contexte)
        CLOUD-->>CG: Texte nettoyé
        CG->>J: Ligne : horodatage, fournisseur, taille, finalité
        Note over J: Jamais de contenu
    end
```

---

## 6. Cycle de vie des modèles et des mises à jour

```mermaid
flowchart LR
    subgraph Modeles
        A1[Choix du modèle<br/>taille et langue affichées] --> A2[Téléchargement avec reprise]
        A2 --> A3{Empreinte SHA-256<br/>valide ?}
        A3 -- non --> A4[Rejet + message + nouvelle tentative]
        A3 -- oui --> A5[Installation dans le dossier modèles]
        A5 --> A6[Chargement vérifié à chaque démarrage]
        A6 --> A7[Suppression propre par l'utilisateur]
    end
    subgraph MisesAJour
        B1[Manifeste signé] --> B2{Vérification minisign}
        B2 -- non --> B3[Refus silencieux + journal local]
        B2 -- oui --> B4[Progression 10% → 50% → 100%]
        B4 --> B5{Erreur au démarrage ?}
        B5 -- oui --> B6[Retour à la version précédente]
        B5 -- non --> B7[Version active]
    end
```

---

## 7. Table des défaillances et des reprises

| Défaillance | Détection | Comportement | Exigence |
|---|---|---|---|
| Micro débranché pendant la prise | Nombre de trames nul / événement de périphérique | Arrêt propre, message, audio déjà capté conservé | RF-10 |
| Micro occupé en mode exclusif | Échec d'ouverture WASAPI | Message explicite + aide (« une autre application utilise le micro ») | RF-08 |
| Silence total (raccourci oublié) | VAD | Aucune insertion, état « rien à transcrire » | RF-18 |
| Erreur du moteur (mémoire, modèle) | Retour d'erreur / panique capturée | Audio conservé, « réessayer », rechargement automatique | RF-20, RF-21 |
| Insertion refusée par la cible | Valeur non relue / pas de changement détecté | Passage au niveau suivant, puis fenêtre de secours | RF-28, RF-31 |
| Champ sensible | `IsPassword` ou liste d'exclusion | Refus, message neutre, aucune lecture | RF-29 |
| Fenêtre cible fermée | Identifiant de fenêtre invalide | Fenêtre de secours | RF-32 |
| Presse-papiers verrouillé par un tiers | Échec d'ouverture du presse-papiers | Réessais avec temporisation, puis niveau 3 | RF-30 |
| Copie utilisateur pendant le maintien | Comparaison du contenu | La copie de l'utilisateur est conservée | RF-30 |
| Fenêtre élevée au premier plan | Détection d'échec d'injection | Message pédagogique, aucun échec silencieux | ADR-06 |
| Réseau coupé avec fonction cloud active | Erreur réseau | Repli automatique en local, information de l'utilisateur | RF-36 |
| Panique du webview | Absence d'événement de vie | Le noyau continue ; le webview est relancé | ADR-02 |
| Disque plein pendant un téléchargement | Erreur d'écriture | Téléchargement interrompu proprement, reprise possible | RF-15 |

---

## 8. Ce qui sort de la machine : tableau de vérité

Ce tableau est la spécification de la page « Qu'est-ce qui sort de ma machine ? » (RF-35).

| Situation | Audio | Texte dicté | Contexte (app/titre/sélection) | Destinataire | Désactivable ? |
|---|---|---|---|---|---|
| Dictée en mode local strict | Non | Non | Non | — | — |
| Dictée + post-traitement cloud consenti | Non | **Oui** | Non | Fournisseur choisi par l'utilisateur | Oui, par dictée et globalement |
| Vérification de mise à jour | Non | Non | Non | Serveur de mise à jour | Oui |
| Téléchargement d'un modèle | Non | Non | Non | Source du modèle | Oui (installation manuelle possible) |
| Activation de licence (facultative) | Non | Non | Non (identifiant de poste haché) | Serveur de licence | Oui (mode hors ligne avec grâce) |
| Télémétrie (facultative) | Non | Non | Non (identifiant rotatif, compteurs) | Serveur de télémétrie | Oui — **désactivée par défaut** |

Aucune autre transmission n'est autorisée par conception. Toute nouvelle transmission exige une
modification de ce tableau, donc une revue explicite.
