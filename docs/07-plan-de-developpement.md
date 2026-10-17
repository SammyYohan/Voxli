# Livrable 7 — Plan de développement progressif, coûts et modèle économique

---

## 1. Principes de progression

1. **Tranche verticale d'abord.** Chaque phase livre un produit utilisable de bout en bout pour un
   cas étroit, jamais une couche horizontale non testable.
2. **Mesurer avant de promettre.** Aucune communication externe (site, prix, comparatif) avant une
   campagne de mesure datée (`docs/06`, §11).
3. **Le mode local et la récupération du texte ne sont jamais reportés.** Ce sont des exigences de
   confiance : les intégrer dès le MVP évite de les ajouter après coup à une architecture cloud.
4. **Le gratuit doit rester utile.** Si la version gratuite est un démo déguisée, la promesse de
   confidentialité et la réputation open source du marché se retournent contre nous.

---

## 2. Phases, durées et critères de sortie

> Les durées supposent **une personne à temps plein** (développement + recette). Elles sont des
> estimations, pas des engagements ; la phase P0 sert précisément à les recalibrer.

| Phase | Durée estimée | Contenu | Critères de sortie | Risque principal |
|---|---|---|---|---|
| **P0 — Spike vertical mesuré** | 2 semaines | Raccourci global → capture WASAPI → VAD minimal → whisper.cpp (`base`/`small`) → insertion (Notepad + Word) ; journalisation de mesure ; script de mesure de latence et de mémoire ; test « aucune connexion sortante » | Les 5 critères de `docs/06` §10 (P0) | L'insertion échoue dans moins de 4 des 6 applications testées → **arrêt et réévaluation** |
| **P1 — MVP interne** | 6–8 semaines | Interface de réglages, gestionnaire de modèles (téléchargement repris et vérifié), VAD (Silero), dictionnaire de base, historique en mémoire, pastille d'état native, profils d'insertion, fenêtre de secours, installeur NSIS non signé | Exigences MVP de `docs/02` §13 ; matrice de 12 applications ≥ 90 % | Complexité de la cascade d'insertion et du presse-papiers |
| **P2 — Confiance et industrialisation** | 4 semaines | Mode local strict prouvé, secrets DPAPI, exclusions, journalisation rédigée, tests S1–S10 en CI, signature de code, mise à jour signée avec retour arrière, documentation « Qu'est-ce qui sort de ma machine ? », politique de vulnérabilités et durée de support | Critères P2 de `docs/06` §10 | Coût et délai d'obtention du certificat de signature ; faux positifs EDR |
| **P3 — Différenciation** | 6–10 semaines | Moteur Parakeet v3 (attribution en place), vocabulaire métier injecté, profils par application, post-traitement local (niveaux 0–1), aperçu en streaming (évaluation), support ARM64 (décision), tests H3/H4/H6 | Critères P3 | Qualité réelle des moteurs sur le français métier (mesure H4) |
| **P4 — Commercialisation** | 4–6 semaines | Licences Ed25519 + parcours d'achat, page de vente honnête, documentation, support outillé, dépôt Microsoft Store (option), préparation CRA | Critères P4 | Conversion réelle (H1) non prouvée avant le lancement |

**Rythme de livraison interne** : une build interne par semaine à partir de P1, une recette
instrumentée par phase minimum, un journal de mesures à chaque changement de modèle.

---

## 3. Périmètre exact du prototype vertical (P0) — pour validation

Ce qui suit est **le prototype minimal demandé**, et rien de plus. Chaque élément non couvert est
listé comme tel.

### 3.1 Dans le périmètre

1. Un exécutable Windows (x64) qui démarre, se place dans la zone de notification et reste discret.
2. Un raccourci global **push-to-talk** (valeur fixe au départ, reconfiguration à chaud hors
   périmètre P0).
3. Capture du microphone par défaut en mode partagé, pré-roll de 400 ms, conversion mono 16 kHz f32.
4. VAD minimal (seuil d'énergie + durée minimale) — le VAD Silero complet arrive en P1.
5. Transcription locale avec whisper.cpp (`base` ou `small`, quantifié) sur CPU.
6. Insertion : niveau 1 (UIA `ValuePattern` si disponible) puis niveau 2 (presse-papiers + `Ctrl+V`
   avec sauvegarde/restauration), puis niveau 4 (fenêtre de secours).
7. Fenêtre de secours affichant le texte, pré-sélectionné, avec « copier » et « coller à nouveau ».
8. Journal de mesure local (latences par étape, mémoire, moteur utilisé, niveau d'insertion) —
   **sans contenu de texte**.
9. Script de mesure : latence (p50/p95 sur 30 énoncés), mémoire (repos/pic), taille de l'installateur.
10. Test automatisé « aucune connexion sortante » (S1, version simplifiée).

### 3.2 Hors périmètre du prototype (et donc **non vérifié** après P0)

- Parakeet v3 / sherpa-onnx (P3), post-traitement (P3), dictionnaires (P1), historique persistant (P1),
  reconfiguration du raccourci (P1), profils par application (P1), gestionnaire de modèles avec pause
  et reprise (P1), VAD Silero (P1), licence et activation (P4), mise à jour signée (P2), signature de
  code (P2), ARM64 (P3), tests de compatibilité étendus (P1), accessibilité complète (P3), télémétrie
  (jamais par défaut).

### 3.3 Chaîne d'outils et commandes prévues (à exécuter en P0)

Prérequis : Rust stable (toolchain `x86_64-pc-windows-msvc`), Visual Studio Build Tools 2022
(« Développement Desktop en C++ » + SDK Windows 10/11), CMake, Node.js LTS, pnpm.

```powershell
# 1. Vérification de l'environnement
rustc --version ; cargo --version ; cmake --version ; node --version ; pnpm --version

# 2. Compilation du noyau seul (tests unitaires hors Windows : machine à états, plan d'insertion)
cargo test -p plume-core

# 3. Récupération d'un modèle de test (une seule fois, depuis le dépôt officiel whisper.cpp)
#    Taille annoncée à l'utilisateur dans l'application ; ici, poste de développement.
.\scripts\fetch-model.ps1 -Model base -Quantization q5_0

# 4. Lancement en développement
pnpm install
pnpm tauri dev

# 5. Banc de latence (rejoue le corpus de recette et écrit un CSV daté)
cargo run --release -p plume-bench -- --corpus tests/fixtures/audio/latency --out benchmarks/latency-$(Get-Date -Format yyyyMMdd).csv

# 6. Mesure mémoire (échantillonnage du processus pendant 30 dictées)
.\scripts\measure-memory.ps1 -DurationSeconds 900 -Out benchmarks\memory-$(Get-Date -Format yyyyMMdd).csv

# 7. Preuve d'absence de connexion sortante (mode local strict, pare-feu local)
.\scripts\prove-no-network.ps1 -Exe .\target\release\plume.exe

# 8. Build de l'installateur (P1 : signé ; P0 : non signé et étiqueté comme tel)
pnpm tauri build
```

### 3.4 Ce que le prototype **ne** prouvera pas

- La compatibilité avec l'ensemble de la matrice d'applications (6 applications sur 12, dont aucune
  application élevée).
- La qualité du français en usage métier (aucun corpus « Plume-FR » à ce stade : il doit être
  enregistré en P1).
- La tenue sur 60 minutes (l'endurance est mesurée en P1).
- La conformité réglementaire ou la robustesse face à un EDR d'entreprise.

Ces limites doivent être écrites dans le rapport de P0, pas seulement dans ce document.

---

## 4. Risques et dépendances (registre de suivi)

| # | Risque / dépendance | Impact | Probabilité | Mitigation | Déclencheur d'alerte |
|---|---|---|---|---|---|
| R1 | Insertion insuffisamment fiable (voir T1) | Critique | Moyenne-élevée | Cascade, profils, fenêtre de secours, recette mesurée ; **décision en P0** | < 90 % de réussite sur la matrice |
| R2 | Blocage par EDR/antivirus en entreprise | Élevée (commercial) | Moyenne-élevée | Mode sans crochet, signature, documentation, canal DSI | Premier signalement d'un client pilote |
| R3 | Latence insuffisante sur M1 | Élevée | Moyenne | Moteur par défaut adapté au matériel, quantification, threads, cibles mesurées avant promesse | p95 > 2,9 s en P0 |
| R4 | Droit à l'attribution CC-BY-4.0 omis | Élevée (juridique) | Faible | Écran de licences, checklist de release | Revue de conformité en P1 |
| R5 | Coût de la signature de code et du Store | Moyenne | Faible | Budgéter avant P2 ; alternative : distribution par canal direct en attendant | Devis obtenu |
| R6 | CRA : obligations de sécurité et de support | Élevée | Élevée (certaine) | Durée de support publiée, politique de vulnérabilités, mises à jour signées, avis juridique | Déclaration de conformité à préparer avant P4 |
| R7 | Dépendance à un composant tiers qui change de licence | Élevée | Faible-moyenne | Politique de licences, choix de composants à licence permissive, capacité à remplacer un moteur (ADR-03) | Changement de licence annoncé en amont |
| R8 | Un pair gratuit (Handy, Voice Access, Microsoft) comble l'écart | Critique (commercial) | Moyenne | Différenciation sur fiabilité, conformité, support, verticales | Une fonctionnalité majeure gratuite sort |
| R9 | Un seul développeur (bus factor 1) | Élevée | Certaine | Documentation (ce dépôt), automatisation de la CI, pas de composant exotique non remplaçable | — |
| R10 | Modèles ou runtimes imposant des mises à jour cassantes | Moyenne | Moyenne | Épingler les versions, tester un cycle de mise à jour complet (S8), journal des changements de modèle | Build rouge après mise à jour |

---

## 5. Modèle de coûts (hypothèses explicites, à réviser)

Toutes les valeurs de cette section sont des **[HYPOTHÈSE]** chiffrées pour discuter, pas des devis.
Devise : euro, hors taxes, ordre de grandeur pour un éditeur individuel (France).

### 5.1 Développement (temps)

| Poste | Estimation | Hypothèse |
|---|---|---|
| P0 | 2 semaines | 1 personne à plein temps |
| P1 | 6–8 semaines | Idem ; +30 % si la cascade d'insertion exige des profils complexes |
| P2 | 4 semaines | Inclut signature, mises à jour, tests de sécurité |
| P3 | 6–10 semaines | Inclut Parakeet, dictionnaires, vocabulaire, mesures |
| P4 | 4–6 semaines | Licences, page de vente, support, Store |
| **Total jusqu'à la première version commercialisable** | **≈ 22–30 semaines** | Si une seule personne, en parallèle d'un autre revenu : doubler le calendrier |

Coût d'opportunité : à valoriser au tarif journalier de la personne (par exemple 400–700 €/jour en
indépendant) — soit **44 000 à 105 000 €** de temps sur la période. C'est le poste dominant et il est
souvent oublié dans les plans « un produit à 30 € ».

### 5.2 Coûts récurrents annuels (à vérifier au moment de l'engagement)

| Poste | Ordre de grandeur | Statut |
|---|---|---|
| Certificat de signature de code (OV/EV ou service de signature géré) | De l'ordre de 100 à 500 €/an selon le type et le fournisseur | **[HYPOTHÈSE]** — tarifs à vérifier (certains fournisseurs ont changé leurs modèles) |
| Compte développeur Microsoft Store | Frais uniques modestes (individuel) | **[HYPOTHÈSE]** à vérifier |
| Hébergement (téléchargements de modèles, mises à jour, licence) | 5–30 €/mois au début ; la bande passante des modèles peut devenir le premier poste si les modèles sont hébergés par nous | **[HYPOTHÈSE]** — héberger les modèles chez un tiers réduit le coût et la responsabilité |
| Support | 0,5–1 h par licence vendue la première année | **[HYPOTHÈSE]** — le support est le poste caché d'un achat unique |
| Outillage (CI, comptes, polices) | 0–40 €/mois | **[HYPOTHÈSE]** |
| Juridique (CGV, mentions, DPA, question CRA) | 500–3 000 € one-shot | **[HYPOTHÈSE]** |

### 5.3 Coût de l'inférence

| Mode | Coût | Remarque |
|---|---|---|
| **Local** | 0 € marginal (coût matériel porté par l'utilisateur : CPU, batterie, RAM) | Avantage structurel du produit : le coût ne grandit pas avec l'usage |
| Cloud (option facultative) | Ordre de 0,15 à 0,60 $ **par heure** d'audio selon le fournisseur et le modèle, à revérifier | **[HYPOTHÈSE]** : les tarifs de transcription changent fréquemment ; à revalider au moment de l'intégration |

Conséquence : l'option cloud doit être **facturée à l'usage ou via la clé personnelle de
l'utilisateur (BYOK)**, sinon un utilisateur intensif peut coûter plus qu'il ne paie. Le mode BYOK
(clé de l'utilisateur) supprime le coût pour nous et le risque de rétention de données chez nous.

---

## 6. Modèle économique : évaluation critique

### 6.1 Les trois options demandées

**A. Offre gratuite.** Nécessaire, mais elle doit avoir une raison d'être claire. Proposition :
transcription locale illimitée avec les modèles légers, historique en mémoire, un dictionnaire,
aucun nettoyage IA avancé. Le gratuit est aussi un **outil de conformité** : il ne coûte rien en
inférence et prouve le positionnement local.

**B. Licence professionnelle à achat unique.** C'est la préférence exprimée dans le brief. Elle est
cohérente avec le marché (25–69 $ chez les petits outils, 249,99 $ chez Superwhisper, 699 $ chez
Dragon) mais **incompatible avec un coût de support illimité** et avec le CRA.
**[RECO]** : licence perpétuelle avec **fenêtre annoncée de mises à jour fonctionnelles** (par
exemple 24 mois), mises à jour de sécurité pendant la durée de support publiée, et remise pour le
passage à la version majeure suivante. C'est lisible, défendable juridiquement, et ça évite la
dérive « licence à vie, support à vie ».

**C. Option IA distante facultative.** À conserver, mais en **BYOK par défaut** (l'utilisateur
fournit sa clé) et/ou en crédits prépayés. Cette option ne doit **jamais** être la condition d'un
résultat correct : elle améliore la forme, elle ne remplace pas le cœur du produit.

### 6.2 Hypothèses de prix à tester (aucune recommandation ferme à ce stade)

| Scénario | Prix | Justification de l'ancrage | À tester par |
|---|---|---|---|
| Gratuit | 0 € | Concurrence gratuite crédible (Handy, Voice Access) | — |
| Pro — achat unique | **39–59 €** | Entre les petits outils (25–69 $) et le milieu de gamme local (Superwhisper 249,99 $ à vie) | H1 : page de vente réelle, précommandes |
| Pro — abonnement | **5–8 €/mois** | Sous Wispr Flow (15 $/mois) en échange d'un périmètre cloud plus réduit | H1 variante B |
| Entreprise | **20–40 €/poste/an** (ou offre unique multi-postes) | Ancre : Dragon Anywhere 55 $/mois, Wispr Growth 23–33 $/utilisateur/mois | H7 : entretiens DSI |

**Avertissement assumé** : ces prix sont des hypothèses de travail construites sur des ancres
concurrentes vérifiées. **Aucune donnée de conversion réelle n'existe à ce stade.** Un prix ne devient
une décision qu'après H1.

### 6.3 Conséquences des mises à jour et du support à long terme

1. Chaque version publiée doit être **supportable** : un achat unique crée une population qui attend
   des correctifs pendant des années. D'où l'importance d'une politique écrite (durée de support par
   version majeure) publiée **avant** la première vente.
2. Les modèles dépendent de tiers (OpenAI, NVIDIA, sherpa-onnx, ONNX Runtime) : une mise à jour de
   modèle peut changer les résultats. Toute mise à jour de modèle doit être mesurée comme un
   changement de produit (campagne de benchmark obligatoire).
3. Le CRA impose de pouvoir réagir à une vulnérabilité : sans chaîne de build reproductible et sans
   processus de release outillé, le coût explose en cas d'incident.
4. Le support d'une application de dictée est **environnemental** (« ça ne marche pas dans mon
   application ») plus que logiciel. Il faut donc, dès P1, un outil d'export de diagnostic **rédigé**
   et un mode de recette qui évite de demander à l'utilisateur de décrire sa configuration.

### 6.4 Verdict économique [RECO]

Le modèle « achat unique » est **viable** à trois conditions : (1) le segment visé est celui qui paie
pour la conformité et le support (segments A et D de `docs/01`), (2) le coût de support est borné par
une politique écrite et un bon outillage de diagnostic, (3) le développement du cœur reste unique —
toute fonctionnalité qui multiplie les configurations multiplie le support. À l'inverse, un produit
grand public à 30 € sans politique de support et sans différenciation forte, qui vise le même
utilisateur que Handy ou Superwhisper, **ne se finance pas**.

---

## 7. Décisions go / no-go

| Jalon | Décision | Critère |
|---|---|---|
| Après P0 | Poursuivre ou arrêter | Réussite d'insertion ≥ 5/6 applications testées **et** latence p95 mesurée ≤ 2,9 s sur M1 **et** aucune connexion sortante en mode strict |
| Après P1 | Poursuivre vers le produit ou rester en outil interne | Matrice de 12 applications ≥ 90 % **et** H3 (taux de repli) mesuré < 5 % sur 10 utilisateurs pilotes |
| Avant P4 | Commercialiser ou diffuser gratuitement | H1 confirmée (≥ 10 précommandes payantes ou ≥ 3 lettres d'intention de 10 postes) |
| Après 6 mois d'exploitation | Maintenir la licence perpétuelle ou passer à l'abonnement | Coût de support réel > 15 % du revenu des nouvelles ventes |

---

## 8. Questions ouvertes à trancher par le commanditaire

1. **Valider les 3 propositions de valeur** (`docs/01` §6) — et confirmer celle qui devient le
   message principal.
2. **Choisir la cible prioritaire** : segment A (professionnels réglementés) ou B (travailleurs du
   savoir). Le choix change la feuille de route P3-P4 (conformité et déploiement contre ouverture de
   l'essai gratuit).
3. **Autoriser le spike P0** (2 semaines) ou demander d'abord une validation marché plus poussée.
4. **Trancher le modèle moteur par défaut** (`docs/03` ADR-03) après le spike.
5. **Confirmer le nom du produit** et la politique de marque (le nom `Plume` est un nom de travail).
6. **Décider de la politique de mise à jour** : fenêtre de mises à jour fonctionnelles incluse dans
   la licence perpétuelle (24 mois proposés) et durée de support de sécurité (24 ou 36 mois).
