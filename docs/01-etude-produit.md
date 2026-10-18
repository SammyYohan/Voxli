# Livrable 1 — Étude produit et concurrentielle

**Date de rédaction : 17 octobre 2026.** Toutes les vérifications de prix, de versions et de
fonctionnalités ont été faites à cette date. Les prix et les fonctions concurrentes bougent vite :
revalider avant toute décision d'investissement.

---

## 0. Méthode et niveaux de confiance

| Étiquette | Signification |
|---|---|
| **[FAIT]** | Vérifié sur source primaire (documentation éditeur, dépôt de code, documentation Microsoft, registre de paquets). URL + date en §10. |
| **[FAIT-INTÉRESSÉ]** | Fait rapporté par un concurrent ou un site d'affiliation à un concurrent. Probablement exact, mais non recoupé par une source neutre. |
| **[HYPOTHÈSE]** | Non testé. Doit être invalidé ou confirmé par une expérience utilisateur ou une mesure. |
| **[RECO]** | Recommandation assumée, avec son raisonnement. |

Trois biais de méthode à garder en tête :

1. **Le paysage concurrentiel de la dictée locale sur Windows est saturé d'outils gratuits** (dont
   au moins un projet MIT à plus de 30 000 étoiles). Toute étude qui prétend que « le marché est
   vide » est fausse.
2. Plusieurs sources trouvées en recherche sont des **pages marketing de concurrents** déguisées en
   comparatifs (voibe.com, spokenly.app, dictaflow.io, saymurmur.app, audexum.com, openwhispr.com).
   Elles sont utilisées uniquement pour des informations auto-déclarées sur **leur propre** produit,
   jamais comme preuve de faiblesse d'un tiers.
3. Le WER (taux d'erreur de mots) est utilisé partout comme argument commercial. Il dépend du
   corpus, de la langue, du bruit et de la ponctuation incluse ou non dans la mesure. **Un WER
   n'est comparable qu'à protocole identique.**

---

## 1. Besoins prioritaires des utilisateurs Windows

### 1.1 Ce que la documentation des concurrents révèle (signal fort)

Les pages d'aide sont contraintes par la réalité : elles documentent ce qui casse. Trois
observations de ce type sont plus informatives que n'importe quel comparatif marketing :

| Observation | Source | Ce que ça révèle |
|---|---|---|
| Le concurrent leader insère le texte **via le presse-papiers**, restaure l'ancien contenu ~0,5 s après, mais **garde le texte au presse-papiers 3 s** pour Outlook/Word/WhatsApp et **5 s** pour VNC/RDP/Citrix/TeamViewer. Il fournit aussi un raccourci « coller la dernière transcription » (Shift+Alt+Z sous Windows). | Centre d'aide Wispr Flow, article « What to expect from Flow: accuracy and known limitations », consulté 17/10/2026 **[FAIT]** | L'**insertion de texte est le problème n°1** de la catégorie, y compris chez le leader financé. Les applications lentes à lire le presse-papiers et le bureau à distance sont les cas d'échec structurels. C'est là que se gagne la fidélité. |
| Le même éditeur documente des échecs d'insertion avec bouton « coller manuellement », des limites de session, et la nécessité de vérifier l'historique si le transducteur a réussi mais l'insertion échoué. | Idem **[FAIT]** | Le **chemin d'échec doit être un produit à part entière** (texte récupérable, jamais perdu), pas une exception à loguer. |
| La dictée intégrée de Windows **désactive automatiquement la correction IA dans les champs de mot de passe**. | Documentation Microsoft via synthèse presse spécialisée, 02/10/2026 **[FAIT-INTÉRESSÉ]** | L'**exclusion des champs sensibles est une attente de base**, pas un argument différenciant. Ne pas la traiter comme un bonus marketing. |

### 1.2 Besoins classés par impact sur la rétention

| Rang | Besoin | Pourquoi maintenant | Statut |
|---|---|---|---|
| 1 | **Le texte atterrit au bon endroit, du premier coup** (tous les logiciels usuels, y compris Outlook, Teams, Word, Chrome, VS Code, Windows Terminal, éditeurs distants) | C'est la douleur n°1 documentée par le leader lui-même ; un échec sur 20 dictées détruit la confiance bien plus qu'un mot erroné | **[FAIT]** pour la douleur, **[HYPOTHÈSE]** pour le seuil d'acceptabilité |
| 2 | **Latence perçue courte** entre le relâchement de la touche et le texte | Les outils concurrents sont perçus « instantanés » ; une latence > 3 s fait revenir au clavier | **[HYPOTHÈSE]** à mesurer (voir H6) |
| 3 | **Confidentialité vérifiable** (audio et texte qui ne sortent pas de la machine) | Secteurs juridique/santé/finance/public ; le leader a introduit un mode local dans son offre, signe que le marché l'exige | **[FAIT]** pour l'offre concurrente, **[HYPOTHÈSE]** pour la disposition à payer |
| 4 | **Fonctionnement hors ligne** | Déplacements, réseaux d'entreprise filtrés, avions, salles blanches | **[FAIT]** (Voice Access et les outils locaux le font déjà, c'est une commodité) |
| 5 | **Vocabulaire métier et noms propres** corrects (termes techniques, noms de clients, acronymes français) | Le coût de correction manuelle annule le gain de la dictée | **[HYPOTHÈSE]** forte, à valider (H4) |
| 6 | **Qualité du français** (ponctuation, accents, accords, nombres, unités) | La plupart des comparatifs sont anglophones ; la qualité annoncée en français varie fortement | **[FAIT]** sur les écarts de WER annoncés (voir §4.3) |
| 7 | **Zéro configuration** au premier lancement | Les outils locaux gratuits exigent souvent de choisir un modèle et un réglage VAD | **[FAIT]** (constaté dans la documentation de Handy et descriptions d'outils locaux) |
| 8 | **Coût prévisible** (pas d'abonnement) sur un poste de travail professionnel | Dragon facture 699 $ en achat unique depuis 25 ans : ce modèle persiste | **[FAIT]** pour l'existence du prix, **[HYPOTHÈSE]** pour l'attachement au modèle |
| 9 | **Confort pour les longues dictées** (30–60 min sans dérive mémoire ni surchauffe) | Cas d'usage « je rédige un document », pas seulement « je réponds à un message » | **[HYPOTHÈSE]** |
| 10 | **Accessibilité** (TDAH, dyslexie, RSI) | Public motivé mais souvent non payeur, largement servi par le gratuit (Handy, Voice Access, Talon) | **[HYPOTHÈSE]** |

---

## 2. Segments de clientèle et potentiel commercial

Le potentiel est estimé par l'**ancre de prix existante** sur le segment (ce que le marché paie
déjà pour un problème voisin), pas par une étude de marché que nous n'avons pas.

| Segment | Taille relative *(estimation qualitative)* | Ancre de prix observée | Verdict |
|---|---|---|---|
| **A. Professionnels de l'écrit réglementé** (avocats, experts-comptables, notaires, médecins, consultants, assurance) | Moyenne | Dragon Professional v16 à **699 $** achat unique ; Dragon Anywhere à **55 $/mois** ; offre « conformité » chez Wispr (SOC 2, ISO 27001, HIPAA, BAA signé) **[FAIT]** | **Meilleur potentiel.** Exigences fortes (confidentialité, vocabulaire, insertion dans Word/Outlook/CRM), disposition à payer prouvée, et la conformité est un fossé défendable. |
| **B. Travailleurs du savoir intensifs** (PM, sales, marketing, support, rédaction) | Grande | Wispr Flow **15 $/mois** ou **144 $/an** ; Superwhisper **8,49 $/mois** / **249,99 $** à vie **[FAIT]** | **Volume.** Mais concurrence frontale : impossible de gagner sur « IA qui reformule », qui est leur terrain. Gagnable sur l'insertion fiable et le prix en achat unique. |
| **C. Développeurs et technique** | Moyenne | Handy (MIT, gratuit), OpenWhispr (MIT, Pro 8 $/utilisateur/mois pour le cloud), Whispering (MIT/AGPL) **[FAIT]** | **Faible monétisation directe.** Fort effet de prescription (recommandation publique), faible disposition à payer pour le local. Utile comme vitrine, pas comme source de revenu. |
| **D. Entreprises avec contrainte de conformité** (DASRI, santé, public, banque, défense) | Petite en nombre, forte en valeur | Dragon Professional/Anywhere en déploiement de masse ; exigences SSO/SCIM/audit chez Wispr Growth/Enterprise (23–33 $/utilisateur/mois) **[FAIT]** | **Meilleure marge unitaire**, mais cycle long, exige MSI + GPO + pas d'admin + DPA + CRA. À viser en année 2 après preuve sur A/B. |
| **E. Accessibilité / RSI** | Moyenne | Gratuit : Voice Access (Windows 11), Talon (**179 $** à vie, mais courbe d'apprentissage de semaines), Handy **[FAIT]** pour les offres | **Non prioritaire en tant que segment payant.** Servir gratuitement : c'est un devoir produit et un bon canal de réputation. |
| **F. Marché francophone spécifiquement** | — | Outils locaux franco-allemands existants (ex. 5 €/mois) **[FAIT-INTÉRESSÉ]** ; la plupart des outils leaders sont anglophones natifs | **Angle crédible** : qualité du français, vocabulaire administratif et technique français, support et documentation en français, conformité RGPD/CRA expliquée en français. |

**Lecture critique [RECO].** Le segment le plus solvable (A/D) demande exactement ce que le
gratuit ne fournit pas : la garantie, la traçabilité, le contrat et le support. Le segment le plus
grand (B) est défendable uniquement sur des critères que les grands acteurs traitent déjà bien.
Construire d'abord pour A, avec une architecture qui permet de servir B sans compromis, puis viser D.

---

## 3. Problèmes concurrents vérifiés

Volontairement factuel et charitable : ces éléments sont des **failles exploitables**, pas des
« défaillances ».

| # | Constat | Source | Date | Confiance |
|---|---|---|---|---|
| 3.1 | L'insertion repose sur le presse-papiers, avec des maintiens longs (3 s Word/Outlook/WhatsApp, 5 s en RDP/VNC/Citrix) et un risque documenté : une application lente peut insérer l'ancien contenu du presse-papiers après la levée du maintien. | Centre d'aide Wispr Flow | consulté 17/10/2026 | **Élevée** (auto-documenté par l'éditeur) |
| 3.2 | L'insertion dépend de l'application (bureau à distance explicitement mentionnés comme lents à lire le presse-papiers). Sur mobile, repli sur le presse-papiers pour les champs complexes. | Centre d'aide Wispr Flow | consulté 17/10/2026 | **Élevée** |
| 3.3 | Une seule langue détectée par dictée (pas par mot) : le code-switching (français/anglais technique, très courant en entreprise) est explicitement moins fiable. | Centre d'aide Wispr Flow | consulté 17/10/2026 | **Élevée** |
| 3.4 | Les dictées longues et enchaînées prennent plus de temps ; sur Android les sessions sont limitées dans le temps. | Centre d'aide Wispr Flow | consulté 17/10/2026 | **Élevée** |
| 3.5 | Le concurrent leader a introduit un « mode local processing » dans son offre — preuve que la confidentialité est devenue une exigence de marché, mais sa structure historique est cloud, et son produit reste dépendant d'un serveur pour l'IA de reformatage. | Page tarifs Wispr Flow (FAQ) | consulté 17/10/2026 | **Moyenne** (formulation du FAQ ; le périmètre exact du local n'est pas spécifié publiquement dans la source consultée) |
| 3.6 | Le concurrent leader ne supporte pas Windows on ARM (Snapdragon X), là où un concurrent local le supporte (avec un seul modèle local disponible sur cette plateforme). | Comparatif tiers détaillé (saymurmur.app) recoupant les docs éditeurs | 15/10/2026 | **Moyenne** (tiers, non confirmé mot pour mot par l'éditeur dans nos sources) |
| 3.7 | Les critiques utilisateurs du leader portent sur la fiabilité et la performance Windows ; note Trustpilot annoncée à 2,7/5. | Blog d'un outil concurrent (spokenly.app) | 2026 | **Faible** (source intéressée, non vérifiée sur Trustpilot) |
| 3.8 | Les outils locaux gratuits sont réels et crédibles, mais ils laissent des trous : pas d'aperçu temps réel, nettoyage IA désactivé par défaut ou exigeant une configuration, modèles récents non intégrés, courbe d'apprentissage (Talon), gestion des modèles laissée à l'utilisateur. | Test comparatif indépendant de 21 alternatives (adamjones.me), README de Handy/OpenWhispr, dépôts publics | 15/04/2026 et lecture des dépôts | **Moyenne-élevée** |
| 3.9 | La dictée intégrée de Windows a deux visages inégalement connus : `Win+H` (cloud, nécessite Internet) et **Voice Access** (Windows 11 22H2+, local, hors ligne, sans limite, gratuit). Sur PC Copilot+ (NPU ≥ 40 TOPS), **Fluid Dictation** ajoute un nettoyage IA local. | Documentation Microsoft et synthèses presse spécialisées | 02/10/2026 et 20/07/2026 | **Élevée** sur l'existence, **moyenne** sur les détails de déploiement |
| 3.10 | Aucune des deux dictées intégrées n'offre de **vocabulaire personnalisé**. | Synthèse presse spécialisée, 20/07/2026 — cohérent avec la documentation Microsoft | 20/07/2026 | **Moyenne-élevée** |
| 3.11 | Dragon Professional v16 est un achat unique à **699 $**, optimisé Windows 11, hors ligne, avec commandes et vocabulaire métier — mais complexe, lourd et sans IA de nettoyage moderne. | Fiche produit Nuance + distributeur CDW | consulté 17/10/2026 | **Élevée** |

**Synthèse des failles exploitables [RECO].**
(a) L'insertion fiable et sans effet de bord sur le presse-papiers est un problème non résolu ;
(b) le français et le vocabulaire métier francophone sont peu traités ;
(c) le bureau à distance et les applications lentes sont un angle mort assumé par le leader ;
(d) le gratuit local existe mais laisse un déficit d'expérience (aperçu, mise en route, réglages) ;
(e) la conformité documentée et commercialisable en français est un espace vide.

---

## 4. Comparaison des solutions existantes

### 4.1 Tableau principal (prix listés en USD, vérifiés le 17/10/2026)

| Solution | Modèle | Plateformes | Traitement | Prix affiché | Points forts | Points faibles / limites |
|---|---|---|---|---|---|---|
| **Wispr Flow** | Commercial, cloud d'abord + mode local revendiqué | Mac, Windows, iOS, Android | Cloud (mode local annoncé) | Gratuit 2 000 mots/semaine (desktop) · Pro 15 $/mois ou 144 $/an · Growth 23–26 $/utilisateur/mois · Enterprise sur devis | Produit le plus poli de la catégorie ; dictionnaire et extraits partagés ; offre conformité (SOC 2, ISO 27001, HIPAA, BAA) ; 100+ langues annoncées ; équipe très financée | Pas d'achat unique ; insertion par presse-papiers avec cas d'échec documentés ; une seule langue par dictée ; dépendance serveur ; ARM Windows non supporté |
| **Superwhisper** | Commercial, local d'abord | Mac, Windows (x64 + ARM64), iOS, Android | Local (+ modèles cloud en Pro) | Gratuit (modèles Whisper locaux illimités) · Pro 8,49 $/mois, 84,99 $/an, **249,99 $ à vie** | Local gratuit généreux ; licence à vie ; modèles variés (Whisper, Parakeet, Cohere Transcribe) ; LLM local de nettoyage (S1-mini) ; modes contextuels | Sur Snapdragon, seul Parakeet tourne en local ; le nettoyage IA est payant ; fonctions avancées verrouillées en Pro |
| **Handy** | Open source MIT | Windows, macOS, Linux | Local uniquement | **Gratuit** | Tauri + Rust, léger ; whisper-rs + Parakeet v3 ; VAD Silero ; GPU (Intel/AMD/NVIDIA) ; > 30 000 étoiles GitHub ; installable via winget | Pas d'aperçu temps réel ; nettoyage IA optionnel à configurer ; pas de gestion de flotte ; marque non redistribuable |
| **OpenWhispr** | Open source MIT + Pro cloud | Windows, macOS, Linux, iOS | Local et cloud | Gratuit · Pro 8 $/utilisateur/mois (cloud) | Écosystème riche (notes, diarisation, agent) ; modèles locaux variés via sherpa-onnx ; BYOK | Electron (empreinte plus lourde) ; complexité fonctionnelle |
| **Dictée Windows `Win+H`** | Intégré | Windows 10/11 | Cloud Microsoft | Gratuit | Disponible partout, zéro installation | Nécessite Internet ; ponctuation automatique désactivée par défaut ; pas de vocabulaire personnalisé |
| **Voice Access (Windows 11 22H2+)** | Intégré | Windows 11 | Local, hors ligne | Gratuit | Hors ligne, sans limite, contrôle du PC par la voix ; Fluid Dictation sur Copilot+ | Windows 11 uniquement ; pas de vocabulaire personnalisé ; pas d'insertion dans certains contextes |
| **Dragon Professional v16** | Commercial, achat unique | Windows 10/11 | Local | **699 $** (1 poste) · Anywhere 55 $/mois | Vocabulaire métier, commandes, macros, transcription de fichiers, déploiement entreprise | Technologie datée, UI lourde, pas d'IA moderne de reformatage, prix élevé |
| **Talon** | Commercial (à vie) + gratuit open source | Windows, macOS, Linux | Local | 179 $ à vie (ou build open source) | Contrôle total du PC, dictée de code | Courbe d'apprentissage de plusieurs semaines |
| **VoiceInk / Dictano / VoiceTypr / Weesper / VoxScribe** | Divers (achat unique 25–69 $, abonnement 5 €/mois, ou gratuit MIT) | Mac surtout, quelques-uns Windows | Local | 25–69 $ à vie ou gratuit | Prix bas, local | Fragmentés, petits volumes, qualité variable, peu de support |

### 4.2 Lecture concurrentielle [RECO]

- **Ne pas se battre sur le terrain de Wispr Flow** (reformatage IA « magique », multiplateforme,
  dictionnaire partagé). Ce terrain est financé à hauteur de 81 M$ et se joue sur la qualité du
  modèle cloud.
- **Se battre sur les trois terrains que le leader documente lui-même comme difficiles** :
  insertion dans les applications réelles (dont bureau à distance), confiance locale et
  conformité, qualité du français et du vocabulaire métier.
- **Accepter la présence du gratuit.** Handy et Voice Access peuvent sembler disqualifiants pour un
  produit payant. En réalité : Voice Access n'a pas de vocabulaire personnalisé, et Handy ne vise
  pas la conformité ni le support. Un produit payant se justifie par la **garantie**, non par la
  seule fonction.
- **Surveiller le risque de plateforme** : si Microsoft généralise Fluid Dictation (nettoyage IA
  local) à toutes les machines Windows 11 et non plus aux seuls PC Copilot+, une partie du
  différenciateur « texte propre » disparaît pour le grand public. Le positionnement B2B/conformité
  reste protégé.

### 4.3 Point technique qui change la donne : le moteur par langue

Mesure publiée par un éditeur de modèles (source intéressée mais protocole explicite : corpus FLEURS,
MLS, Common Voice 23, WER normalisé), consultée le 17/10/2026 **[FAIT-INTÉRESSÉ]** :

| Modèle | WER français annoncé | WER anglais annoncé | Licence | Exécution CPU |
|---|---|---|---|---|
| whisper-large-v3-turbo (OpenAI) | 7,97 % | 4,80 % | MIT | Oui, mais coûteuse |
| Parakeet TDT 0.6B v3 (NVIDIA) | **5,39 %** | 4,85 % | CC-BY-4.0 | **Oui, optimisée (ONNX)** |
| Canary 1B v2 (NVIDIA) | 4,86 % | 4,70 % | (à vérifier) | Moins pratique |

**Conséquence produit [RECO]** : pour un produit francophone et européen, un moteur Whisper seul
n'est probablement pas optimal — ni en précision française, ni en coût CPU. Dual-moteur
(Whisper pour la couverture linguistique large, Parakeet v3 pour les 25 langues européennes dont le
français) est une décision d'architecture motivée, pas un gadget. Voir `docs/03`, ADR-03.
**Attention licence** : CC-BY-4.0 impose l'attribution visible à NVIDIA dans le produit.

---

## 5. Risques

### 5.1 Techniques

| # | Risque | Gravité | Probabilité | Mitigation |
|---|---|---|---|---|
| T1 | **L'insertion fiable dans toutes les applications est un problème ouvert** (presse-papiers lents, champs non standard, terminaux, Electron, jeux DirectInput, applications élevées bloquées par UIPI) | Critique | Élevée | Cascade d'insertion en 4 niveaux ; profils par application ; chemin d'échec toujours récupérable ; matrice de compatibilité testée à chaque release. **À démontrer en P0.** |
| T2 | **Un champ élevé (admin) bloque à la fois les raccourcis et l'injection de texte** (isolation UIPI de Windows) | Élevée | Moyenne | Ne jamais exiger l'élévation ; détecter et informer clairement ; documenter la limite. Aucun concurrent n'y échappe. |
| T3 | Hallucinations de Whisper sur le silence ou l'audio non vocal | Élevée | Élevée | VAD en amont, seuil de probabilité « no speech », longueur minimale d'audio, filtre par motifs connus, test dédié (RF-18) |
| T4 | Latence inacceptable sur CPU bas de gamme (mesures publiques : `small` int8 à ~1,7–2,0 s par phrase courte sur un Ryzen 5 3600XT — soit un ordre de grandeur trop lent pour une conversation) | Élevée | Élevée | Parakeet v3 CPU en moteur par défaut sur les machines sans GPU ; cibles de performance mesurées avant promesse commerciale |
| T5 | Explosion de la taille des modèles et du premier lancement (téléchargements de 75 Mo à 1,5 Go) | Moyenne | Élevée | Liste de modèles avec taille affichée, reprise, vérification d'intégrité, mode « démarrer maintenant, télécharger en arrière-plan » |
| T6 | Dépendance à WebView2 (Runtime Microsoft Edge) | Faible | Faible | Préinstallé sur Windows 11 et Windows 10 à jour ; bootstrapper embarqué en option (~1,8 Mo) ; mode installateur hors ligne si nécessaire |
| T7 | Faux positifs antivirus/EDR : un crochet clavier + injection d'entrées ressemble à un keylogger | Élevée en entreprise | Moyenne-élevée | Limiter l'usage des crochets (préférer `RegisterHotKey`) ; fournir un mode « sans crochet » ; signature de code ; documentation fournisseur ; canal de déblocage pour les DSI |
| T8 | Qualité audio dégradée par les casques Bluetooth (passage en profil HFP mono-16 kHz à l'ouverture du micro) | Moyenne | Élevée | Pré-roll, messagerie utilisateur pédagogique, mesure de niveau, documentation du comportement |
| T9 | Windows on ARM (Snapdragon X) | Moyenne | Moyenne | Cibles ARM64 dès P1 si Parakeet/ONNX est le moteur principal ; sinon annoncé comme non supporté |
| T10 | Incertitude de performance GPU sur machines réelles (pilotes Vulkan/CUDA, iGPU) | Moyenne | Moyenne | CPU-first, GPU en accélération optionnelle détectée, jamais en prérequis |

### 5.2 Commerciaux

| # | Risque | Gravité | Mitigation |
|---|---|---|---|
| C1 | **Le gratuit est crédible** : Handy (MIT), Voice Access (intégré), VoxScribe (MIT) couvrent l'essentiel du besoin | Critique | Ne pas vendre la fonction brute : vendre la fiabilité mesurée, le support, la conformité, le déploiement, la garantie de mise à jour |
| C2 | Un leader financé (81 M$) et multiplateforme occupe l'esprit du marché | Critique | Niche francophone/européenne + conformité + achat unique |
| C3 | Microsoft intègre davantage d'IA de dictée localement | Élevée | Se différencier sur ce que Microsoft ne fera pas : vocabulaire métier, intégration métier, support, secteurs réglementés |
| C4 | Coût d'acquisition élevé sur un marché d'outils à faible prix | Moyenne | Vente par verticales, contenu comparatif honnête, essai sans friction, licence à vie en tête d'affiche |
| C5 | Support d'un produit payant (attentes plus élevées que sur le gratuit) | Moyenne | Périmètre de support contractuel clair ; base de connaissance ; pas de promesse à 24/7 |

### 5.3 Juridiques et réglementaires

| # | Risque | Détail | Mitigation |
|---|---|---|---|
| J1 | **RGPD** — la voix est une donnée personnelle ; le texte dicté peut contenir des données sensibles (santé, judicial) ; une dictée peut enregistrer des tiers | Base légale, minimisation, DPA avec les prestataires, cas d'usage médical à encadrer (HDS en France) | Mode local par défaut, aucune rétention, chiffrement au repos de l'historique, registre de traitement, DPA, doctrine documentée **[FAIT]** que l'obligation s'applique, **[RECO]** sur les mesures |
| J2 | **Cyber Resilience Act (UE 2024/2847)** — un logiciel installé localement est un « produit comportant des éléments numériques » : marquage CE, exigences de sécurité, gestion des vulnérabilités, notification d'incidents sous 24 h / 72 h | Obligations de notification **applicables depuis le 11/09/2026** ; **application complète le 11/12/2027** ; les produits déjà sur le marché y entrent en cas de modification substantielle | À budgéter **avant** la commercialisation en UE : politique de vulnérabilités, documentation technique, mises à jour de sécurité pendant la fenêtre de support, conformité au marquage CE **[FAIT]** |
| J3 | **Licences des composants** — whisper.cpp MIT, poids Whisper MIT, Parakeet v3 **CC-BY-4.0 (attribution obligatoire)**, whisper-rs sous Unlicense (bindings) | L'attribution n'est pas optionnelle ; l'omettre invalide la licence du modèle | Écran « À propos / Licences » listant les attributions, y compris NVIDIA **[FAIT]** |
| J4 | **Licence perpétuelle vs. coûts récurrents** — un achat unique implique de maintenir et sécuriser le produit pendant des années (voir J2) | Contradiction structurelle du modèle « achat unique » | Licence perpétuelle **avec fenêtre de mises à jour fonctionnelles limitée (ex. 2 ans)** ; mises à jour de sécurité pendant la durée de support annoncée **[RECO]** |
| J5 | **AI Act (UE)** — obligations de transparence pour les systèmes d'IA ; une application de dictée est un risque faible, mais la divulgation de l'usage d'IA dans les documents produits peut devenir contractuelle chez certains clients | Pas de blocage identifié à ce stade | Analyse juridique dédiée avant lancement B2B ; ne pas promettre une conformité non auditée |
| J6 | **TVA/OSS** sur la vente de licences numériques dans l'UE | Obligations fiscales dès la première vente à un particulier | Prestataire de paiement gérant la TVA (merchant of record) recommandé |
| J7 | **Accessibilité (European Accessibility Act)** pour certains services numériques | Périmètre discutable pour un outil de productivité | Vérifier avec un conseil si vente au grand public UE |

---

## 6. Les trois propositions de valeur les plus différenciantes

Classées par défendabilité (et non par attractivité marketing).

### V1 — « Vos mots restent sur votre poste. C'est vérifiable, pas déclaratif. »
Le mode local est un **mode strict** : aucune fonction distante n'est appelée, l'application
fonctionne avec le réseau coupé, et cette affirmation est vérifiée automatiquement dans la chaîne
d'intégration (test qui échoue si une socket sort). Chaque éventuelle transmission est listée,
nommée, consentie et journalisée dans l'interface.
*Pourquoi c'est différenciant :* les concurrents doivent ménager leur architecture cloud ; nous
n'avons rien à ménager. *Pourquoi c'est vendable :* c'est la question n°1 des acheteurs en santé,
droit, finance et public — et la réponse est auditable par leur DSI.

### V2 — « Le texte atterrit là où vous écrivez, du premier coup — ou vous le récupérez toujours. »
Insertion en cascade mesurée (UIA → presse-papiers avec restauration → frappe Unicode → fenêtre de
secours), hygiène du presse-papiers (pas de pollution de l'historique Windows), profils par
application, gestion explicitement soignée du bureau à distance et des applications lentes, et
toujours un chemin « coller à nouveau ».
*Pourquoi c'est différenciant :* c'est le problème que le leader documente lui-même comme non
résolu (3 s / 5 s de maintien, échecs possibles). C'est aussi la douleur quotidienne qui fait
abandonner un outil.

### V3 — « Il connaît votre vocabulaire, et il ne réécrit pas vos phrases. »
Dictionnaire par utilisateur et par profil applicatif, injection du vocabulaire dans le moteur
(`initial_prompt`/hotwords), post-traitement **désactivé par défaut**, à trois niveaux (règles
locales, petit modèle local, cloud sur consentement), avec un mode verbatim garanti et un test de
non-régression sémantique.
*Pourquoi c'est différenciant :* les outils intégrés n'ont pas de vocabulaire personnalisé
(limite identifiée dans les sources) ; les outils « IA » ont tendance à reformuler, ce qui est
inacceptable pour un juriste, un médecin ou un développeur. « Corrige sans trahir » est une promesse
claire, vérifiable et opposable.

**Différenciateur secondaire à garder sous le coude** : le français et les langues européennes
traités comme des langues de première classe (moteur Parakeet v3, ponctuation et typographie
françaises, support et documentation en français).

---

## 7. Hypothèses à tester avant de développer le produit complet

| # | Hypothèse | Test | Seuil de décision | Coût |
|---|---|---|---|---|
| H1 | Des professionnels paient un achat unique de 39–69 € pour une dictée locale fiable | Page de vente réelle (« faux portail ») + 15 entretiens + précommandes | ≥ 8 % de visiteurs qualifiés qui laissent un e-mail, ≥ 10 précommandes payantes | Faible |
| H2 | La confidentialité est le premier critère d'achat pour les segments A/D | Test A/B de message (privacy d'abord vs. fiabilité d'abord vs. prix d'abord) sur trafic qualifié | Le message privacy surperforme les autres en taux de clic **et** en intention déclarée | Faible |
| H3 | La fiabilité d'insertion est le facteur n°1 de rétention | Instrumentation opt-in du taux de repli (niveau 3/4 de la cascade) + journal d'usage sur 10 utilisateurs pendant 2 semaines | < 2 % de replis sur les 12 applications cibles | Moyen |
| H4 | Le vocabulaire métier français est mal servi par les modèles par défaut | Constituer un corpus FR métier (60 énoncés) et mesurer le WER avec et sans dictionnaire | Gain ≥ 5 points de WER avec dictionnaire sur les termes métier | Faible |
| H5 | Les utilisateurs acceptent un téléchargement initial de ~150–500 Mo | Analyse d'entonnoir du premier lancement, 100 installations | ≥ 70 % des installations aboutissent à une première dictée réussie | Faible |
| H6 | La latence perçue acceptable est < ~2 s entre la fin de la parole et l'insertion | Étude perceptuelle : 20 utilisateurs, latences imposées (0,8 / 1,5 / 2,5 / 4 s), note de fluidité | Identification d'un seuil reproduit à ±0,5 s | Moyen |
| H7 | Le segment D exige MSI + GPO + installation sans droits admin + audit | 5 entretiens DSI/acheteurs | ≥ 3/5 citent spontanément ≥ 2 de ces 4 exigences | Faible |
| H8 | Le français est un critère de choix suffisant pour se distinguer sur ce marché | 10 entretiens acheteurs francophones + test de message | ≥ 5/10 citent la qualité du français comme critère décisif | Faible |

**Règle d'arrêt :** si H1 et H3 échouent simultanément, le produit tel que conçu n'a pas de marché
identifié — arrêter ou pivoter avant d'engager le développement complet.

---

## 8. Ce que je conteste dans le prompt initial (devoir d'esprit critique)

Le brief demande « valider le produit **avant** de coder », puis « ensuite, un prototype vertical
minimal ». L'intention est saine, mais l'ordonnancement est risqué : les deux inconnues les plus
décisives du produit ne se valident pas sur le papier.

1. **La fiabilité d'insertion ne se démontre pas en amont, elle se mesure.** Aucune étude
   concurrentielle ne dira si *notre* cascade fonctionne dans Outlook classique, Teams, VS Code,
   Citrix et un champ administrateur. **[RECO]** : lancer un spike technique de 2 semaines **en
   parallèle** de la validation produit, sur 12 applications cibles, et faire dépendre la décision
   d'investissement de ses mesures. Le prompt demande de ne pas coder tout de suite ; il ne demande
   pas de s'interdire 2 semaines de mesure.
2. **« Achat unique » et « conformité » sont en tension.** Le Cyber Resilience Act impose une
   gestion des vulnérabilités et des mises à jour, potentiellement pendant des années, sur un
   produit vendu une fois. Vendre une licence perpétuelle sans fenêtre de support bornée, c'est
   signer une dette non chiffrée. **[RECO]** : « licence perpétuelle + 24 mois de mises à jour
   fonctionnelles + mises à jour de sécurité pendant la durée de support publiée ».
3. **« 100 % local » est une allégation fragile si elle est mal cadrée.** Téléchargement des
   modèles, vérification de mise à jour, contrôle de licence : trois flux réseau légitimes. Une
   allégation imprécise est attaquable et décrédibilise tout le discours. **[RECO]** : une page
   « Qu'est-ce qui sort de ma machine ? » qui liste ces flux, et une formulation testée (RF-35).
4. **Tauri n'apporte pas les gains de mémoire souvent annoncés *pour ce produit*** : le shell pèse
   quelques dizaines de Mo de webview, le modèle ASR pèse 60 Mo à 1,5 Go. Le choix de Tauri se
   justifie par la vélocité et l'écosystème, pas par la RAM. **[RECO]** : ne pas communiquer sur la
   mémoire sans mesures.
5. **Le prix ne doit pas être recommandé sans données.** Aucune donnée de conversion n'existe à ce
   stade. Le prix est une **hypothèse** (H1), avec pour seule ancre observable : 8,49 $/mois et
   249,99 $ à vie chez Superwhisper, 25–69 $ à vie chez les petits outils locaux, 699 $ chez Dragon.
   Toute communication interne doit présenter ces chiffres comme des ancres, pas comme un prix optimal.
6. **Le prompt suppose que Wispr Flow est la concurrence principale.** Sur Windows
   spécifiquement, la concurrence la plus dangereuse est peut-être **gratuite** : Handy (MIT,
   > 30 000 étoiles) et Voice Access (intégré, hors ligne, sans limite). La stratégie de
   différenciation doit répondre au gratuit avant de répondre au payant.

---

## 9. Recommandation de séquence

1. Spike P0 (2 semaines) : mesures de latence, de fiabilité d'insertion et de mémoire sur 3 machines.
2. En parallèle : tests H1, H2, H4 (coût faible, décisions fortes).
3. Décision go/no-go sur la base des mesures, pas des impressions.
4. P1→P4 comme décrit dans `docs/07`.

---

## 10. Sources

Toutes consultées le **17 octobre 2026**.

| # | Source | Type | Usage |
|---|---|---|---|
| S1 | https://wisprflow.ai/pricing — page tarifs officielle | Primaire | Prix, plans, fonctions, FAQ mode local |
| S2 | https://docs.wisprflow.ai/articles/4048537120 — « What to expect from Flow: accuracy and known limitations » | Primaire (éditeur) | Insertion par presse-papiers, maintiens 3 s/5 s, une langue par dictée, échecs d'insertion |
| S3 | https://docs.wisprflow.ai/articles/9559327591 — « Flow plans and what's included » | Primaire (éditeur) | Détail des plans |
| S4 | https://superwhisper.com/docs/billing/plans.md + https://superwhisper.com/windows + changelog | Primaire (éditeur) | Prix 8,49 $/mois, 84,99 $/an, 249,99 $ à vie, gratuit local, ARM64, modèles |
| S5 | https://github.com/cjpais/handy — README et dépôt | Primaire (code) | Handy : MIT, Tauri/Rust, whisper-rs, Parakeet v3, VAD Silero, winget, nombre d'étoiles |
| S6 | https://openwhispr.com/compare/handy et https://github.com/cpiprint/open-wispr | Primaire/intéressée | Écosystème des outils open source, sherpa-onnx, Pro 8 $/utilisateur/mois |
| S7 | https://adamjones.me/blog/best-dictation-apps-2026/ | Tiers indépendant | Test comparatif de 21 alternatives ; lacunes des outils libres (aperçu temps réel, réglages) |
| S8 | https://www.getvoibe.com/resources/... et https://spokenly.app/..., https://saymurmur.app/..., https://dictaflow.io/..., https://audexum.com/... | Concurrentes/intéressées | Faiblesses revendiquées du leader (fiabilité, Trustpilot 2,7/5, pas d'ARM) — **confiance faible** |
| S9 | Documentation Microsoft UI Automation (ValuePattern, TextPattern, UIPI) | Primaire (Microsoft) | Contraintes d'insertion : ValuePattern mono-ligne uniquement, TextPattern ne permet pas d'écrire, UIPI bloque l'injection inter-privilèges |
| S10 | https://raw.githubusercontent.com/tauri-apps/global-hotkey/... + changelog global-shortcut | Primaire (code) | Raccourci global = `RegisterHotKey` + scrutation `GetAsyncKeyState` toutes les 50 ms pour la détection de relâchement ; échec si la combinaison est prise |
| S11 | https://crates.io/api/v1/crates/{tauri,whisper-rs,cpal,tauri-plugin-global-shortcut} | Primaire (registre) | Versions réelles au 17/10/2026 : Tauri 2.12.4 (3.0.0-alpha.5 en préversion), whisper-rs 0.16.0, cpal 0.18.2, plugin global-shortcut 2.4.1 |
| S12 | https://github.com/ggml-org/whisper.cpp (LICENSE, README) | Primaire (code) | Licence MIT, support Vulkan/CUDA/OpenVINO, taille et quantifications des modèles |
| S13 | https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3 | Primaire (éditeur du modèle) | Licence CC-BY-4.0, 25 langues européennes, ponctuation et casse automatiques, usage commercial autorisé |
| S14 | https://github.com/TheStageAI/TheWhisper/blob/main/benchmark/README.md | Article scientifique/intéressée | Tableau de WER multilingue (français : Parakeet 5,39 / turbo 7,97 / Canary 4,86) |
| S15 | https://www.nuance.com/dragon/... + https://www.cdw.com/... | Primaire + distributeur | Dragon Professional v16 à 699 $, Anywhere à 55 $/mois |
| S16 | https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act (+ pages notification et mise en œuvre) | Primaire (Commission européenne) | CRA : en vigueur, notification depuis le 11/09/2026, application complète le 11/12/2027, logiciels installés localement dans le périmètre |
| S17 | Synthèses presse spécialisées sur la dictée Windows (voiceos.com, weesperneonflow.ai, geekrewind.com, voxbooster.com) | Tiers | `Win+H` cloud, Voice Access local, Fluid Dictation sur Copilot+, absence de vocabulaire personnalisé |
| S18 | https://github.com/drajb/whisper-local, https://voxscribe-ai-voice-dictation.vercel.app/ | Primaire (code) | État de l'art des fonctionnalités attendues : pré-roll de 500 ms, « coller la dernière dictée », fenêtre de secours, mode verbatim |
| S19 | https://v2.tauri.app/distribute/windows-installer/ | Primaire (Tauri) | Modes d'installation WebView2 : bootstrapper (~2 Mo), hors ligne (~127 Mo), version figée (~180 Mo) |
