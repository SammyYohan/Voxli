# Livrable 6 — Plan de tests, protocole de benchmark et critères de validation

Objectif : rendre **falsifiables** toutes les promesses du produit. Aucun chiffre de ce document n'est
un résultat : chaque ligne est un protocole, avec l'outil, la méthode et le format de restitution.

---

## 1. Stratégie de test

| Niveau | Ce qui est couvert | Outil | Fréquence |
|---|---|---|---|
| **Unitaire (hors Windows)** | Machine à états, plan d'insertion (fonction pure), conversion audio, normalisation, politique de conflits de raccourcis, vérification de licence, VAD sur fichiers | `cargo test` / `cargo nextest` | À chaque commit, en CI |
| **Intégration noyau** | Pipeline complet avec `FakeEngine` et `FakeCapture` : silence, erreur moteur, file d'attente, focus perdu, consentement absent | `cargo test --features testkit` | À chaque commit |
| **Intégration réelle** | whisper.cpp avec modèle `tiny`/`base` sur corpus WAV de recette ; Parakeet sur 3 énoncés ; insertion réelle dans Notepad pilotée par UIA | Runner Windows (GitHub Actions `windows-latest` ou machine dédiée) | À chaque PR |
| **Recette manuelle instrumentée** | Matrice de 12 applications, modes dégradés, parcours de consentement, licences | Journal de recette (`tests/e2e/*.md`) | Avant chaque release |
| **Mesure de performance** | Latence, mémoire, taille, WER, stabilité longue | Bancs dédiés (`docs/06` §3-6) | Avant chaque release, et à chaque changement de modèle |
| **Sécurité et vie privée** | Aucune connexion en mode strict, aucune fuite dans les journaux, falsification de licence, permissions, dépendances | Tests automatisés + `cargo audit` / `cargo deny` | À chaque release |

**Ce qui n'est pas testable automatiquement et restera une recette manuelle** : le rendu visuel, le
confort de la latence perçue, le comportement dans les applications propriétaires fermées, l'usage
réel sur des postes équipés d'antivirus d'entreprise. Ces points doivent être **explicitement
signés** dans la recette, jamais supposés.

---

## 2. Parc de machines de référence

| Profil | Caractéristiques | Pourquoi |
|---|---|---|
| **M1 — portable milieu de gamme** | Intel i5-1135G7 (4c/8t), 16 Go, SSD, iGPU, Windows 11 23H2 | Cas le plus fréquent ; c'est **la** machine de référence pour les cibles de latence |
| **M2 — tour performante** | Ryzen 7 5800X (8c/16t), 32 Go, NVMe, RTX 3060, Windows 11 24H2 | Mesure du chemin GPU (Vulkan/CUDA) et des longues dictées |
| **M3 — machine ancienne** | Intel i3-8100, 8 Go, HDD/SATA SSD, Windows 10 22H2 | Seuil de viabilité ; décide des valeurs par défaut (moteur, modèle, threads) |
| **M4 — ARM64** | Snapdragon X Elite (ou x86 émulé à défaut), Windows 11 | Décide si ARM64 est annoncé comme supporté ou explicitement non supporté |

Les mesures sont restituées **par profil**, jamais agrégées en un chiffre unique. Une moyenne
multi-plateformes masque exactement ce qui compte : le matériel bas de gamme.

---

## 3. Protocole — taille et mémoire

### 3.1 Taille

| Mesurande | Méthode | Restitution |
|---|---|---|
| Installateur NSIS (`.exe`) | Taille du fichier de release | Mo, arrondi au Mo |
| Installateur MSI | Idem | Mo |
| Empreinte installée (hors modèles) | Comparaison avant/après installation, fichiers sous `Program Files` + `%LOCALAPPDATA%\Plume` hors `models\` | Mo |
| Modèles (par modèle) | Taille téléchargée affichée et réelle | Mo par modèle, dans un tableau |
| Empreinte mémoire disque du cache | Max observé après 20 dictées | Mo |

**Cible initiale (à démontrer)** : installateur ≤ 40 Mo, empreinte hors modèles ≤ 60 Mo.
*Ces cibles sont des objectifs d'ingénierie, pas des engagements.*

### 3.2 Mémoire

| Moment | Protocole | Cible initiale |
|---|---|---|
| Repos, moteur non chargé | 5 min après démarrage, écran de réglages fermé, mesure du RSS du processus et de ses enfants (WebView2 inclus) | ≤ 120 Mo |
| Repos, moteur chargé | Idem, après préchargement du modèle par défaut | ≤ 120 Mo + taille du modèle en mémoire |
| Pic pendant transcription | Échantillonnage toutes les 250 ms pendant 30 dictées de 15 s, valeur maximale | ≤ taille modèle + 300 Mo |
| Après 500 dictées | Mesure du RSS après 500 dictées enchaînées | Croissance ≤ 50 Mo vs. état de repos (détection de fuite) |
| Modèle déchargé après inactivité | Après 10 min d'inactivité avec l'option active | Retour au niveau « repos, moteur non chargé » ± 20 Mo |

**Outils** : `Get-Process` + compteur de mémoire privée, Process Explorer pour l'arbre des processus,
et une instrumentation interne (journal des allocations majeures) pour attribuer la consommation.
Le RSS seul est trompeur (pages partagées du WebView2) : restituer **mémoire privée** et **ensemble
de travail** séparément.

---

## 4. Protocole — latence

### 4.1 Définition du mesurande

`L = t(texte visible dans le champ cible) − t(fin de la parole)`

- `t(fin de la parole)` est déterminé par une **prise de référence** : l'instant du relâchement du
  raccourci, mesuré par l'instrumentation interne (visible dans le journal de mesure).
- Le point d'arrivée est observé **à l'écran** (vidéo à 120 im/s, analyse image par image) sur
  10 échantillons, afin de **valider l'instrumentation interne** (écart accepté < 100 ms). Ensuite,
  les mesures de routine utilisent l'instrumentation interne.

### 4.2 Corpus et conditions

- 30 énoncés français de 3, 10 et 25 secondes (10 de chaque), lus naturellement.
- 3 conditions : bureau calme, bruit de fond de bureau ouvert, casque Bluetooth (profil mains libres).
- 3 configurations moteur : modèle par défaut CPU, modèle Whisper turbo, Parakeet v3 int8.

### 4.3 Restitution

Un tableau par machine de référence et par configuration : **p50, p95, maximum**, plus la
décomposition par étape (relâchement, VAD, inférence, post-traitement, insertion).
Aucune moyenne unique. Les valeurs hors ligne (premier lancement à froid) sont mesurées à part.

**Cibles initiales (à démontrer)** : p95 ≤ 2,9 s sur M1 pour un énoncé de 10 s, configuration par
défaut ; p95 ≤ 1,2 s sur M2 avec accélération GPU.

---

## 5. Protocole — taux d'erreur de transcription (WER)

### 5.1 Corpus

| Corpus | Contenu | Licence / usage | Rôle |
|---|---|---|---|
| FLEURS `fr_fr` (extrait de 300 énoncés) | Lecture, propre | Corpus public de recherche | Comparabilité avec les chiffres publiés |
| Common Voice `fr` (extrait de 300 énoncés) | Locuteurs variés, accents, bruit | CC0 | Réalisme des accents |
| **Interne « Plume-FR »** (60 énoncés, à enregistrer) | Vocabulaire métier (juridique, santé, technique), noms propres, nombres, unités, sigles, code-switching FR/EN | Consentement écrit des locuteurs, non diffusé | **Le seul corpus qui prédit l'usage réel** |
| Silence et bruit (20 fichiers de 10 s) | Silence complet, clavier, ventilateur, musique | Interne | Mesure des hallucinations (exigence RF-18) |

### 5.2 Méthode

- Normalisation documentée (casse, ponctuation, nombres écrits en chiffres vs lettres) :
  **la même pour tous les moteurs**, publiée avec les résultats.
- WER global + WER sur les **termes métier uniquement** (mesure spécifique de RF-24).
- Aucun réglage sur le corpus de test : le corpus interne est scindé (30 énoncés de réglage,
  30 énoncés de mesure, jamais utilisés pour le réglage).

### 5.3 Restitution

Tableau `moteur × modèle × corpus → WER`, avec date, version du moteur, quantification, machine, et
la mention explicite des chiffres **non mesurés par nous** (chiffres éditeurs) conservés séparément.

*Ne jamais publier un WER comme une garantie commerciale : c'est une mesure datée, sur un corpus daté.*

---

## 6. Protocole — stabilité et endurance

| Test | Protocole | Critère de succès |
|---|---|---|
| Dictée longue | 60 min d'audio continu (fichier de recette rejoué) en 120 dictées enchaînées | Aucune panique, croissance mémoire ≤ 50 Mo, latence p95 stable (± 20 %) |
| Enchaînement | 1 000 dictées de 5 s automatisées | Aucune perte de texte, aucun blocage du pipeline |
| Veille/reprise | Mise en veille et reprise du poste pendant une session, 20 fois | Le micro est réouvert, le raccourci fonctionne, aucun état bloqué |
| Changement de session | Verrouillage de session, changement d'utilisateur, 5 fois | Aucun raccourci ne fuit entre sessions ; les données restent cloisonnées par utilisateur |
| Batterie | 1 h d'usage type (20 dictées) sur portable non branché | Consommation mesurée et publiée ; pas de maintien du GPU en éveil après 2 min d'inactivité |
| Erreurs provoquées | Modèle supprimé à chaud, disque plein, micro débranché, réseau coupé | Chaque cas produit un comportement prévu (§7 de `docs/04`) |

---

## 7. Matrice de compatibilité (recette obligatoire avant release)

| # | Application | Type de champ | Méthode attendue | Critère |
|---|---|---|---|---|
| 1 | Notepad (Windows 11) | Contrôle simple | ValuePattern (niveau 1) | Insertion correcte, annulation possible par `Ctrl+Z` |
| 2 | Word (classique) | Édition riche | Presse-papiers (niveau 2) | Insertion, style conservé, presse-papiers restauré |
| 3 | Outlook (classique) | Édition riche | Presse-papiers, maintien 3 s | Insertion complète, aucun ancien contenu collé |
| 4 | Outlook (nouveau) | WebView | Presse-papiers | Idem |
| 5 | Teams | WebView | Frappe ou presse-papiers | Insertion, focus conservé |
| 6 | Chrome — Gmail | `contenteditable` | Presse-papiers | Insertion avec retours à la ligne corrects |
| 7 | Chrome — Google Docs | Surface d'édition propriétaire | Presse-papiers | Insertion ; documenter toute limite constatée |
| 8 | Slack (desktop) | Electron | Frappe ou presse-papiers | Insertion sans doublon |
| 9 | VS Code | Electron, éditeur de texte | Profil « verbatim », frappe | Symboles et indentation non reformatés |
| 10 | Windows Terminal | Console (ConPTY) | Presse-papiers (mode collage configuré) | Insertion ; comportement documenté selon la configuration de collage de l'utilisateur |
| 11 | Connexion Bureau à distance (mstsc) | Fenêtre distante | Presse-papiers, maintien 5 s | Insertion après restauration ; latence mesurée spécifiquement |
| 12 | Notion (desktop) | Electron | Presse-papiers | Insertion, aucun échec silencieux |
| ○ | Application élevée (contrôle négatif) | Fenêtre admin | **Échec attendu** | Message explicite, texte récupérable (comportement conforme à ADR-06) |

**Systèmes d'exploitation testés** : Windows 10 22H2, Windows 11 23H2, Windows 11 24H2 (ou la
branche alors en support). Chaque ligne est testée sur M1 au minimum ; les lignes 3, 4, 10, 11 et le
contrôle négatif sont testées sur les 4 profils.

---

## 8. Modes dégradés

| Situation | Comportement attendu | Test |
|---|---|---|
| Aucun GPU / pilotes Vulkan absents | Bascule CPU transparente, avertissement informatif une seule fois | Démarrage sur M3, journal vérifié |
| Réseau indisponible | Aucune erreur bloquante ; le mode local fonctionne ; les fonctions distantes indiquent « indisponible » | Carte réseau désactivée, 10 dictées |
| Aucun microphone | Message guidé vers les réglages Windows | Micro désactivé dans Windows |
| WebView2 absent | L'installateur le fournit ou explique la marche à suivre | Machine virtuelle Windows fraîche, mode « skip » du bootstrapper |
| ARM64 sans support moteur | Message clair, aucun plantage ; le produit reste utilisable avec l'émulation si un moteur y tourne | M4 |
| Espace disque insuffisant | Refus propre du téléchargement de modèle, message, suggestion de modèle plus petit | Disque saturé artificiellement |
| Processus en cours d'arrêt forcé | Aucun fichier corrompu à la reprise (config, historique, modèles) | `taskkill /F` pendant chaque phase critique, 10 fois |

---

## 9. Tests de sécurité et de vie privée

| # | Test | Méthode | Critère de réussite |
|---|---|---|---|
| S1 | Aucune connexion en mode local strict | (a) test automatisé : le client HTTP est un trait dont l'implémentation est absente en mode strict et un test vérifie son absence ; (b) exécution sous règle de pare-feu Windows bloquant **toutes** les sorties, 20 dictées ; (c) capture réseau `netsh trace`/Wireshark archivée comme artefact de recette | Aucune connexion sortante observée ; fonctionne intégralement |
| S2 | Aucune fuite de contenu dans les journaux | 200 dictées contenant des chaînes sentinelles uniques ; analyse par motif de l'ensemble des fichiers de données, base d'historique, fichiers temporaires, dumps | Zéro occurrence |
| S3 | Secrets non extractibles | Recherche de motifs (clés PEM, jetons) dans les artefacts de build, dans `config.json`, dans le stockage du webview ; extraction de chaînes du binaire | Aucun secret, aucune clé privée |
| S4 | Falsification de licence | Jeu de 12 licences (valide, expirée, autre produit, octet altéré, signature rejouée, champ modifié) | Seule la valide est acceptée ; échec sans panique |
| S5 | Champ sensible | 10 champs de mot de passe et 3 invites UAC | Aucune lecture, aucune insertion, message neutre |
| S6 | Hygiène du presse-papiers | 8 scénarios (RF-30) | Restauration exacte, historique Windows non pollué, copies concurrentes préservées |
| S7 | Dépendances | `cargo audit` + `cargo deny` (licences et vulnérabilités) en CI | Aucune vulnérabilité critique connue non traitée ; toutes licences conformes à la politique |
| S8 | Mise à jour | Artefact non signé, signature erronée, manifeste falsifié, retour arrière | Refus propre, aucun binaire non signé exécuté |
| S9 | Moindre privilège | Exécution par un compte standard, inspection des droits demandés | Aucune élévation, aucune écriture hors des dossiers utilisateur |
| S10 | Désinstallation | Après désinstallation : fichiers, registre, modèles, historique | Système propre ; les éléments conservés le sont sur demande explicite |

---

## 10. Critères de sortie par phase

| Phase | Critères de sortie (tous obligatoires) |
|---|---|
| **P0 — spike** | ① dictée locale complète fonctionnant dans Notepad et Word ; ② latence p95 mesurée sur M1 pour 10 s d'audio ; ③ taille et mémoire mesurées ; ④ preuve d'absence de connexion sortante ; ⑤ matrice d'insertion à 6 applications avec verdict par application. **Décision go/no-go.** |
| **P1 — MVP** | RF du périmètre MVP passant leurs tests ; matrice de 12 applications ≥ 90 % de réussite ; cibles de latence atteintes sur M1 ; zéro fuite de journal ; installateur signé installable sur les 3 versions de Windows |
| **P2 — confiance** | S1 à S10 passés ; guide « Qu'est-ce qui sort de ma machine ? » publié et exact ; politique de vulnérabilités et durée de support publiées ; autopsie d'au moins un cycle de mise à jour complet |
| **P3 — différenciation** | Dictionnaires et vocabulaire mesurés (H4) ; profils par application validés ; tests H3 et H6 clos avec résultats documentés |
| **P4 — commercialisation** | Licence et droits testés ; parcours d'achat/activation/révocation ; support outillé ; conformité réglementaire instruite |

---

## 11. Restitution des mesures

- Un dossier `benchmarks/` dans le dépôt, un fichier Markdown **daté** par campagne.
- Chaque campagne contient : version du produit (commit), matériel, système, corpus (empreintes des
  fichiers), configuration, résultats bruts (CSV), interprétation, et **la liste de ce qui n'a pas
  été testé**.
- Toute communication externe cite la campagne correspondante. Une mesure sans campagne datée n'est
  pas publiable.

---

## 12. Ce que ce plan ne garantit pas (limites assumées)

1. Le comportement des applications propriétaires peut changer avec leurs mises à jour : la matrice
   doit être rejouée à chaque release majeure des applications cibles, pas seulement à la nôtre.
2. Les mesures de WER de tiers (éditeurs de modèles) ne sont pas reproductibles par nous sans leur
   protocole exact ; elles sont citées comme telles, jamais fusionnées avec nos chiffres.
3. La latence perçue n'est pas la latence mesurée : seule l'étude perceptuelle H6 tranche le seuil
   d'acceptabilité.
4. Aucun test ne couvre le comportement d'un EDR agressif ; la validation en entreprise exige un test
   sur un poste géré par une DSI.
