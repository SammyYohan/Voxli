# Livrable 2 — Exigences fonctionnelles et critères d'acceptation

Chaque exigence porte un identifiant stable (`RF-xx`), un comportement attendu, un critère
d'acceptation **vérifiable** et un test. Les tests automatisables sont marqués **[AUTO]**, les tests
manuels structurés **[MANUEL]** (protocole décrit, résultat consigné dans un journal de recette).
La priorité suit le plan de développement : **MVP** (P1), **V1** (P2–P3), **V2** (P4 et au-delà).

Rappel : un critère d'acceptation est un **seuil de refus**, pas une promesse commerciale. Les
cibles chiffrées de performance vivent dans `docs/06-plan-de-tests.md` et doivent être mesurées
avant toute communication.

---

## 1. Raccourcis globaux

### RF-01 — Raccourci push-to-talk configurable sans redémarrage
- **Comportement attendu** : l'utilisateur choisit une combinaison (modificateurs + touche). La
  modification est enregistrée et active en moins de 500 ms, sans redémarrer l'application ni le
  moteur ASR.
- **Critère d'acceptation** : après changement, l'ancienne combinaison ne déclenche plus rien et la
  nouvelle déclenche immédiatement ; aucune fenêtre de réglages n'a besoin d'être rouverte.
- **Test** : T-01 **[MANUEL]** — 10 changements consécutifs, dont un retour à la valeur initiale,
  vérifiés à chaque fois dans 2 applications différentes.
- **Priorité** : MVP.

### RF-02 — Trois modes de déclenchement
- **Comportement attendu** : (a) maintenu (push-to-talk), (b) bascule (appui pour démarrer, appui
  pour arrêter), (c) double appui pour verrouiller, configurable.
- **Critère d'acceptation** : dans chaque mode, l'enregistrement démarre et s'arrête exactement une
  fois par appui ; aucun enregistrement orphelin après 60 s d'inactivité ; le mode est mémorisé
  entre les sessions.
- **Test** : T-02 **[AUTO]** (machine à états unitaire, `docs/05`) + **[MANUEL]** sur les 3 modes.
- **Priorité** : MVP (a et b), V1 (c).

### RF-03 — Détection de conflit à l'enregistrement
- **Comportement attendu** : si la combinaison est déjà prise par Windows ou une autre application,
  l'interface l'annonce avant validation et propose des alternatives ; aucun état incohérent n'est
  laissé derrière (pas de crash, pas de raccourci « fantôme »).
- **Critère d'acceptation** : tenter d'enregistrer `Win+L` (réservé par Windows) ou une combinaison
  déjà utilisée par l'application elle-même produit un message explicite et aucun effet de bord.
  *(Le cas « une autre application détient la combinaison » a historiquement provoqué un plantage
  dans l'écosystème utilisé : une version ancienne du plugin levait une panique ; corrigé depuis
  — mais un test dédié reste obligatoire.)*
- **Test** : T-03 **[MANUEL]** avec une seconde application enregistrant la même combinaison via
  `RegisterHotKey`, puis **[AUTO]** sur le chemin d'erreur du wrapper.
- **Priorité** : MVP.

### RF-04 — Raccourcis secondaires
- **Comportement attendu** : « coller la dernière transcription » et « afficher la dernière
  transcription » disposent de raccourcis *distincts* (défaut proposé : `Shift+Alt+Z` et
  `Shift+Alt+X`), désactivables, reconfigurables.
- **Critère d'acceptation** : après une insertion, `Coller la dernière transcription` insère à
  nouveau le même texte dans le champ actif ; si aucun champ n'est actif, le texte est placé dans la
  fenêtre de secours, déjà sélectionné.
- **Test** : T-04 **[MANUEL]** — 5 scénarios (champ actif, aucun champ, application fermée entre
  temps, presse-papiers modifié entre-temps, redémarrage de l'application).
- **Priorité** : MVP.

### RF-05 — Appui pendant une opération en cours
- **Comportement attendu** : si l'utilisateur appuie sur le raccourci pendant une transcription ou
  une insertion, le comportement est défini et visible : soit mise en file (file d'attente de 1),
  soit refus avec indication visuelle. Pas de mélange de deux dictées.
- **Critère d'acceptation** : dicter A, relancer immédiatement pendant la transcription de A →
  les deux textes sont insérés dans l'ordre, jamais entrelacés ni perdus.
- **Test** : T-05 **[AUTO]** sur la machine à états + **[MANUEL]** 10 répétitions.
- **Priorité** : V1.

---

## 2. Capture audio

### RF-06 — Démarrage de capture rapide et sans perte du premier mot
- **Comportement attendu** : un pré-roll de 300 à 500 ms d'audio est conservé en mémoire en
  permanence ; l'audio précédant l'appui est inclus dans la transcription.
- **Critère d'acceptation** : sur 20 énoncés commençant immédiatement après l'appui, aucun premier
  mot tronqué (vérification par écoute du WAV de recette et par comparaison du texte attendu).
- **Test** : T-06 **[MANUEL]** avec corpus dédié de 20 énoncés « premier mot court » (ex. « Oui »,
  « Bon », « Deux »).
- **Priorité** : MVP.

### RF-07 — Capture non bloquante
- **Comportement attendu** : la capture vit sur un thread dédié temps réel ; le callback audio
  n'alloue pas, ne prend pas de verrou long et n'appelle jamais le webview ; l'interface reste
  fluide pendant l'enregistrement.
- **Critère d'acceptation** : pendant une dictée de 5 minutes, aucune perte d'échantillon détectée
  (compteur de trames cohérent avec la durée), aucune micro-saccade UI observée, aucune allocation
  dans le callback (vérification par revue de code et compteur d'allocations en test).
- **Test** : T-07 **[AUTO]** (compteur de trames et de discontinuités) + revue de code.
- **Priorité** : MVP.

### RF-08 — Choix du microphone et suivi du périphérique par défaut
- **Comportement attendu** : au premier lancement, le micro par défaut du système est utilisé. Le
  suivi du changement de périphérique par défaut est actif (déconnexion d'un casque USB, passage à
  un autre micro). Un test de niveau avec affichage du signal est fourni.
- **Critère d'acceptation** : débrancher le micro en cours d'utilisation ne fait pas planter
  l'application ; l'état est annoncé à l'utilisateur ; le test de niveau réagit en moins de 200 ms.
- **Test** : T-08 **[MANUEL]** — 6 scénarios (USB, jack, Bluetooth, périphérique par défaut changé
  dans Windows, micro désactivé dans les paramètres, micro occupé par une autre application).
- **Priorité** : MVP.

### RF-09 — Indicateur d'enregistrement visible et discret
- **Comportement attendu** : un retour visuel non intrusif (pastille flottante ou icône de zone de
  notification + niveau) indique : enregistrement, transcription en cours, insertion, erreur. Le
  retour ne vole jamais le focus du champ actif.
- **Critère d'acceptation** : le champ en cours d'édition ne perd pas le focus lors de l'apparition
  de l'indicateur ; l'utilisateur distingue les 4 états sans consulter de menu ; l'indicateur est
  masquable.
- **Test** : T-09 **[MANUEL]** — vérification du focus via un outil d'inspection UIA, 5 applications.
- **Priorité** : MVP (états enregistrement/insertion), V1 (le reste).

### RF-10 — Changement de périphérique pendant l'enregistrement
- **Comportement attendu** : comportement défini et documenté. Choix retenu : **poursuivre sur le
  périphérique initial** (aucune coupure), avertir l'utilisateur, ne pas migrer l'audio au milieu
  d'une prise.
- **Critère d'acceptation** : débrancher le micro initial en pleine dictée produit soit une
  transcription complète sur l'audio déjà capté, soit un arrêt propre avec message ; jamais un
  fichier corrompu ni un crash.
- **Test** : T-10 **[MANUEL]** — 10 répétitions de débranchement à des instants variés.
- **Priorité** : V1.

### RF-11 — Durée maximale de sécurité
- **Comportement attendu** : durée d'enregistrement continue plafonnée (défaut 5 min,
  configurable 30 s–30 min), avec avertissement à 30 s de la fin puis arrêt et transcription.
- **Critère d'acceptation** : un raccourci oublié en mode bascule n'enregistre pas indéfiniment ;
  l'audio capté est transcrit et inséré normalement.
- **Test** : T-11 **[AUTO]** (horloge simulée) + **[MANUEL]** 1 fois à 30 s.
- **Priorité** : V1.

---

## 3. Conversion du signal

### RF-12 — Conversion correcte vers le format du moteur
- **Comportement attendu** : tout format d'entrée pris en charge (8 kHz à 192 kHz, 1 à 8 canaux,
  entier ou flottant) est converti en **mono, 16 kHz, float32**, avec un ré-échantillonnage
  anti-repliement (filtre), pas une interpolation linéaire naïve.
- **Critère d'acceptation** : un WAV de référence ré-échantillonné par notre chaîne présente un
  rapport signal/bruit et un taux de distorsion harmonique **équivalents à ceux d'une référence
  `soxr`/`rubato`** (écart de SNR < 1 dB) ; test de non-régression sur 3 fréquences (440 Hz,
  1 kHz, 8 kHz) et 2 formats d'entrée.
- **Test** : T-12 **[AUTO]** — comparaison A/B sur fichiers de test générés, plus un test
  d'intelligibilité (WER sur un échantillon ré-échantillonné vs. original, écart < 0,5 point).
- **Priorité** : MVP.

### RF-13 — Robustesse des formats d'entrée
- **Comportement attendu** : aucune hypothèse codée en dur sur le taux d'échantillonnage ou le
  nombre de canaux du périphérique.
- **Critère d'acceptation** : la chaîne fonctionne avec 5 périphériques simulés de
  caractéristiques différentes (48 kHz stéréo flottant, 44,1 kHz stéréo entier 16 bits,
  16 kHz mono, 96 kHz stéréo, 8 kHz mono).
- **Test** : T-13 **[AUTO]** — tests unitaires paramétrés sur des flux synthétiques.
- **Priorité** : MVP.

---

## 4. Transcription locale, hors ligne, erreurs

### RF-14 — Transcription strictement locale par défaut
- **Comportement attendu** : en mode local, la transcription s'exécute sur la machine. Aucune
  fonction distante n'est appelée, même en cas d'erreur. Le produit fonctionne avec la carte réseau
  désactivée.
- **Critère d'acceptation** : sur une machine avec pare-feu bloquant toute sortie, une dictée
  complète aboutit ; **aucune** connexion sortante n'est observée côté machine pendant la dictée
  (test par capture réseau).
- **Test** : T-14 **[AUTO]** — test d'intégration avec sockets bloquées via pare-feu local +
  inspection réseau ; test de non-régression dans la chaîne CI.
- **Priorité** : MVP.

### RF-15 — Gestion des modèles
- **Comportement attendu** : liste de modèles avec **taille affichée avant téléchargement**, reprise
  après interruption, vérification d'intégrité (empreinte), emplacement configurable, suppression, et
  possibilité d'installer un modèle fourni manuellement (fichier local).
- **Critère d'acceptation** : couper le réseau à 50 % d'un téléchargement puis le rétablir →
  téléchargement repris sans repartir de zéro ; un fichier de modèle corrompu (octets modifiés) est
  refusé avec un message clair ; aucun modèle partiel n'est jamais chargé par le moteur.
- **Test** : T-15 **[AUTO]** (téléchargement simulé, corruption d'octets) + **[MANUEL]**.
- **Priorité** : MVP.

### RF-16 — Langue
- **Comportement attendu** : langue par défaut = français ; choix d'une langue forcée ou détection
  automatique ; le choix est visible et modifiable à chaud.
- **Critère d'acceptation** : une dictée en français avec la langue forcée sur `fr` produit la
  ponctuation et les accents français ; la détection automatique n'est jamais utilisée sur un
  énoncé de moins de 1,5 s (risque d'erreur documenté et mesuré ailleurs) — comportement testé.
- **Test** : T-16 **[AUTO]** (règle de garde) + **[MANUEL]** corpus FR/EN.
- **Priorité** : MVP.

### RF-17 — Priorité et réactivité du moteur
- **Comportement attendu** : l'inférence s'exécute dans un thread de travail à priorité *sous* la
  normale ; le nombre de threads est configurable ; l'interface et la capture audio restent
  réactives ; une transcription peut être annulée.
- **Critère d'acceptation** : pendant une transcription longue, la zone de notification répond et la
  capture audio démarre en moins de 100 ms si l'utilisateur redicte ; l'annulation interrompt le
  moteur en moins de 500 ms.
- **Test** : T-17 **[MANUEL]** instrumenté (mesure de latence de reprise).
- **Priorité** : V1.

### RF-18 — Silence et hallucinations
- **Comportement attendu** : le silence, le bruit de fond et l'audio non vocal ne produisent pas de
  texte. Un énoncé inférieur à un seuil minimal (défaut 250 ms de voix détectée) est rejeté sans
  insertion.
- **Critère d'acceptation** : **10 essais de 10 secondes de silence complet produisent 0 insertion**
  avec le moteur par défaut ; 10 essais de bruit ambiant (clavier, ventilateur) produisent au plus
  1 insertion non vide, et ce texte est signalé comme douteux ; les motifs d'hallucination connus
  (« Merci d'avoir regardé cette vidéo », etc.) sont filtrés.
- **Test** : T-18 **[AUTO]** — corpus de silence et de bruit enregistrés, rejeu dans la chaîne CI.
- **Priorité** : **MVP** (c'est un critère de confiance, pas de confort).

### RF-19 — Préchargement du moteur
- **Comportement attendu** : le modèle est chargé (warmup) au démarrage de l'application ou à la
  première interaction, en tâche de fond, sans bloquer la première dictée.
- **Critère d'acceptation** : la première dictée n'est pas plus lente que la deuxième à plus de
  20 % près ; aucun gel de l'interface pendant le chargement.
- **Test** : T-19 **[AUTO]** — mesure de la latence des 5 premières dictées après démarrage à froid.
- **Priorité** : V1.

---

## 5. Détection d'erreur et reprise

### RF-20 — Aucune perte silencieuse
- **Comportement attendu** : si la transcription échoue (erreur moteur, mémoire insuffisante,
  fichier de modèle illisible), l'audio de la prise est conservé (mémoire, puis fichier temporaire
  chiffré si nécessaire) et l'utilisateur dispose d'une action « réessayer » ; le texte déjà transcrit
  n'est jamais perdu.
- **Critère d'acceptation** : provoquer une erreur moteur (modèle renommé à chaud) pendant une
  dictée → notification, audio récupérable, relance du moteur, « réessayer » produit le texte.
- **Test** : T-20 **[MANUEL]** structuré (4 pannes provoquées) + **[AUTO]** sur la machine à états.
- **Priorité** : **MVP**.

### RF-21 — Résilience du moteur
- **Comportement attendu** : une panique ou une erreur fatale du moteur est interceptée ; le moteur
  est rechargé automatiquement ; au bout de 3 échecs consécutifs, l'utilisateur est informé avec un
  message actionnable (changer de modèle, réinstaller le modèle, envoyer un rapport).
- **Critère d'acceptation** : après une panique simulée du moteur, l'application reste utilisable et
  une dictée suivante aboutit.
- **Test** : T-21 **[AUTO]** — injection d'une erreur fatale dans un test d'intégration.
- **Priorité** : V1.

### RF-22 — Récupération du texte indépendamment de l'insertion
- **Comportement attendu** : tout texte transcrit est placé dans un tampon interne consultable
  (fenêtre de secours et/ou historique) **avant** toute tentative d'insertion.
- **Critère d'acceptation** : si l'insertion échoue totalement, l'utilisateur récupère le texte en
  moins de 5 secondes via le raccourci dédié ou la fenêtre ; le texte reste accessible après un
  redémarrage de l'application (si l'historique est activé).
- **Test** : T-22 **[MANUEL]** — 5 scénarios d'échec d'insertion provoqués.
- **Priorité** : **MVP**.

---

## 6. Dictionnaires et vocabulaire

### RF-23 — Dictionnaire personnalisé
- **Comportement attendu** : liste de mots/expressions avec variantes et casse (ex. « SIRET »,
  « Qonto », « arthroscopie »), appliquée après transcription par correspondance floue tolérante aux
  erreurs phonétiques ; import/export CSV ; dictionnaire par profil applicatif.
- **Critère d'acceptation** : un terme du dictionnaire mal transcrit (ex. « casse » au lieu de
  « Qonto ») est corrigé dans ≥ 90 % des cas sur un corpus de 20 énoncés par terme ; aucun
  remplacement non demandé d'un mot banal.
- **Test** : T-23 **[AUTO]** — corpus annoté avec et sans dictionnaire.
- **Priorité** : V1.

### RF-24 — Vocabulaire technique injecté dans le moteur
- **Comportement attendu** : les termes métier sont fournis au moteur comme amorce (`initial_prompt`
  ou liste de mots-clés selon le moteur) sans dégrader la dictée courante.
- **Critère d'acceptation** : sur 60 énoncés métier français, le taux d'erreur sur les **termes
  métier** baisse d'au moins 30 % par rapport à la même configuration sans amorce, sans que le WER
  global se dégrade de plus de 0,5 point.
- **Test** : T-24 **[AUTO]** — protocole dans `docs/06`, corpus FR métier (H4).
- **Priorité** : V1.

### RF-25 — Profils par application
- **Comportement attendu** : un profil (langue, style verbatim/ponctué, post-traitement,
  dictionnaire, formatage) est associé à une application, avec un profil par défaut.
- **Critère d'acceptation** : dicter dans un éditeur de code avec le profil « code » ne produit
  aucune reformulation et conserve les symboles dictés ; dicter dans un client de messagerie avec le
  profil « email » applique la ponctuation et la casse.
- **Test** : T-25 **[MANUEL]** — 6 applications × 2 profils.
- **Priorité** : V2.

---

## 7. Post-traitement

### RF-26 — Post-traitement désactivé par défaut, à trois niveaux
- **Comportement attendu** : niveau 0 = aucun post-traitement (texte brut du moteur). Niveau 1 =
  règles locales déterministes (ponctuation française, espaces insécables des unités, casse en début
  de phrase, suppression des hésitations courantes fr/en). Niveau 2 = petit modèle **local** de
  nettoyage. Niveau 3 = service distant, **jamais activé sans consentement explicite et par
  utilisation**.
- **Critère d'acceptation** : le niveau par défaut est 0 ou 1 (jamais 2/3) ; le passage au niveau 3
  exige une action explicite de l'utilisateur et un libellé indiquant ce qui sera transmis.
- **Test** : T-26 **[AUTO]** (état par défaut) + **[MANUEL]** (parcours de consentement).
- **Priorité** : V1 (niveaux 0–1), V2 (2–3).

### RF-27 — Conservation du sens
- **Comportement attendu** : le post-traitement améliore la forme sans ajouter, supprimer ou altérer
  une information. Le mode « verbatim » garantit un texte identique à la sortie du moteur.
- **Critère d'acceptation** : sur un corpus de 100 énoncés annotés, un relecteur humain juge le sens
  modifié dans **moins de 1 %** des cas pour chaque niveau activé ; aucune insertion d'information
  inventée (noms, nombres, dates) — vérification automatique des nombres et entités présents dans
  l'original.
- **Test** : T-27 **[AUTO]** (conservation des nombres/entités) + **[MANUEL]** (100 énoncés,
  1 relecteur, protocole aveugle).
- **Priorité** : V2.

---

## 8. Insertion du texte dans Windows

### RF-28 — Cascade d'insertion à quatre niveaux
- **Comportement attendu** : dans l'ordre — (1) UI Automation `ValuePattern` si le champ focalisé
  est un contrôle simple, éditable et **non sensible** ; (2) presse-papiers + `Ctrl+V` avec
  restauration ; (3) frappe caractère par caractère via `SendInput` (Unicode, sans presse-papiers) ;
  (4) fenêtre de secours. Le niveau utilisé est journalisé localement (sans contenu).
- **Critère d'acceptation** : sur la matrice de 12 applications cibles (`docs/06`), **100 %** des
  insertions aboutissent au bon endroit, dont au moins 10/12 sans passer par le presse-papiers ou
  avec une restauration correcte du presse-papiers d'origine. Le taux de recours au niveau 4 est
  inférieur à 2 % sur le corpus de recette.
  *(Note : le niveau 1 n'existe que pour les contrôles simples ; les contrôles multilignes
  n'exposent pas `ValuePattern` et les documents riches ne permettent pas l'écriture par
  `TextPattern` — c'est une contrainte de l'API Windows, pas un choix.)*
- **Test** : T-28 **[MANUEL]** instrumenté sur la matrice d'applications + **[AUTO]** sur les
  transitions de la cascade.
- **Priorité** : **MVP**.

### RF-29 — Exclusion des champs et applications sensibles
- **Comportement attendu** : aucune insertion ni lecture de contexte dans un champ de mot de passe
  (`IsPassword`), dans une application de la liste d'exclusion (gestionnaires de mots de passe,
  banque, bureau à distance, écran sécurisé UAC), ni lorsque la fenêtre est en mode sécurisé.
- **Critère d'acceptation** : sur 10 champs de mot de passe (Windows, navigateur, gestionnaire de
  mots de passe, application métier), aucune insertion, aucun texte capturé, et l'utilisateur voit
  un message neutre (« insertion ignorée ici ») ; lors d'une invite UAC, aucun raccourci ne se
  déclenche.
- **Test** : T-29 **[MANUEL]** + **[AUTO]** (règle de décision unitaire sur les propriétés UIA).
- **Priorité** : **MVP**.

### RF-30 — Hygiène du presse-papiers
- **Comportement attendu** : (a) le contenu d'origine du presse-papiers est restauré après
  insertion, y compris pour les formats riches (texte, HTML, RTF) ; (b) l'insertion **ne pollue pas**
  l'historique du presse-papiers Windows ni la synchronisation cloud quand c'est possible grâce aux
  formats de contrôle prévus par Windows ; (c) si l'utilisateur copie quelque chose pendant la
  fenêtre de maintien, **sa copie est conservée** ; (d) un presse-papiers verrouillé par une autre
  application est géré par réessai, sans échec silencieux.
- **Critère d'acceptation** : 8 scénarios de test (§`docs/06` T-30) passent, dont : presse-papiers
  vide, presse-papiers texte, presse-papiers HTML/RTF, presse-papiers volumineux (> 10 Mo),
  copie concurrente pendant l'insertion, application cible lente à lire le presse-papiers,
  `Win+V` (historique) qui ne contient pas la dictée, deux dictées successives rapides.
- **Test** : T-30 **[MANUEL]** + **[AUTO]** sur le composant de sauvegarde/restauration isolé.
- **Priorité** : **MVP**.

### RF-31 — Chemin d'échec explicite
- **Comportement attendu** : si l'insertion échoue, notification non bloquante avec « copier »,
  « coller à nouveau » et « ouvrir la fenêtre de secours ».
- **Critère d'acceptation** : chaque échec est signalé ; l'utilisateur peut récupérer le texte en une
  action ; aucun cas d'échec silencieux n'est possible (test de couverture des branches).
- **Test** : T-31 **[AUTO]** (branches) + **[MANUEL]**.
- **Priorité** : **MVP**.

### RF-32 — Cible mémorisée au moment de la dictée
- **Comportement attendu** : la fenêtre cible est mémorisée **au démarrage de la dictée**, pas au
  moment de l'insertion ; si le focus a changé entre-temps, comportement configurable : insérer dans
  la fenêtre mémorisée (défaut) ou dans la fenêtre actuelle, avec indication.
- **Critère d'acceptation** : dicter dans Word, cliquer sur Teams pendant la transcription →
  le texte va dans Word (défaut) et l'utilisateur en est informé ; le réglage inverse fonctionne.
- **Test** : T-32 **[MANUEL]** — 5 scénarios de changement de focus.
- **Priorité** : V1.

---

## 9. Historique et effacement

### RF-33 — Historique optionnel et maîtrisé
- **Comportement attendu** : désactivé par défaut ? **Non — activé mais en mémoire seule par
  défaut**, avec option « conserver sur disque ». Quand la conservation est activée : stockage local
  chiffré, rétention configurable (défaut 30 jours), suppression unitaire, suppression totale,
  export, purge à la désinstallation.
- **Critère d'acceptation** : en mode « mémoire seule », aucune trace de texte n'existe sur disque
  après arrêt (vérification par recherche de motifs dans les fichiers de données) ; en mode
  « conserver », les enregistrements sont illisibles sans le compte Windows qui les a créés
  (chiffrement lié à l'utilisateur).
- **Test** : T-33 **[AUTO]** — analyse du répertoire de données après 20 dictées avec motifs
  recherchés ; **[MANUEL]** pour la purge à la désinstallation.
- **Priorité** : V1.

### RF-34 — Journaux sans contenu
- **Comportement attendu** : les journaux ne contiennent jamais le texte dicté, l'audio, ni le
  contenu du presse-papiers. Ils contiennent : horodatage, identifiants d'événements, durées,
  moteur, modèle, code d'erreur, niveau d'insertion.
- **Critère d'acceptation** : sur 200 dictées de test contenant des chaînes sentinelles uniques,
  aucune occurrence de ces chaînes n'apparaît dans les journaux ou les fichiers d'état ; test
  automatisé exécuté dans la chaîne CI avec un analyseur de motifs.
- **Test** : T-34 **[AUTO]** — test de fuite par chaînes sentinelles.
- **Priorité** : **MVP**.

---

## 10. Confidentialité et consentement

### RF-35 — Mode local strict et vérifiable
- **Comportement attendu** : un mode « Local strict » garantit qu'aucune donnée (audio, texte,
  contexte) ne quitte la machine ; les seuls flux réseau autorisés dans ce mode sont explicitement
  listés dans l'interface (vérification de mise à jour, téléchargement de modèles, contrôle de
  licence si applicable), et chacun est désactivable. Une page « Qu'est-ce qui sort de ma machine ? »
  documente chaque flux, sa fréquence et son contenu.
- **Critère d'acceptation** : en mode local strict, avec mises à jour et licence désactivées, une
  session de 30 minutes (20 dictées) ne produit **aucune connexion sortante** (preuve par capture
  réseau + test automatisé qui échoue si une socket est établie).
- **Test** : T-35 **[AUTO]** — test d'intégration réseau en CI ; **[MANUEL]** capture avec un outil
  d'analyse réseau, consignée comme artefact de recette.
- **Priorité** : **MVP** (l'exigence, pas la page marketing).

### RF-36 — Consentement explicite et journal des transmissions
- **Comportement attendu** : toute fonction distante (nettoyage cloud, modèle cloud) est désactivée
  par défaut, activable une par une, avec un texte indiquant **ce qui part, vers qui, combien de
  temps c'est conservé**, et révocable. Un journal local liste les transmissions effectuées
  (horodatage, fournisseur, taille, finalité) — sans contenu.
- **Critère d'acceptation** : refuser le consentement laisse le produit pleinement fonctionnel en
  local ; révoquer une fonction supprime immédiatement la possibilité de l'appeler ; le journal est
  consultable et exportable.
- **Test** : T-36 **[MANUEL]** structuré + **[AUTO]** (absence d'appel réseau si consentement absent).
- **Priorité** : V1.

### RF-37 — Périmètre de contexte limité et transparent
- **Comportement attendu** : par défaut, **aucune** lecture du contenu des fenêtres. Si l'option
  « contexte » est activée : lecture limitée au nom de l'application, au titre de la fenêtre, et au
  texte **sélectionné** s'il est disponible via UIA ; l'utilisateur peut voir à tout moment ce qui a
  été lu (aperçu), et les applications exclues ne sont jamais lues.
- **Critère d'acceptation** : avec l'option désactivée, aucun appel UIA de lecture de texte n'est
  effectué (vérification par instrumentation) ; avec l'option activée, l'aperçu montre exactement ce
  qui a été lu, et rien de plus (comparaison avec la fenêtre).
- **Test** : T-37 **[AUTO]** (instrumentation) + **[MANUEL]** (aperçu).
- **Priorité** : V1 (désactivé par défaut en MVP ; l'option arrive en V1).

---

## 11. Licences et fonctionnalités payantes

### RF-38 — Licence locale signée, sans secret embarqué
- **Comportement attendu** : la vérification de licence est **hors ligne** : un fichier de licence
  signé (Ed25519) est vérifié avec une **clé publique** embarquée dans le binaire ; aucune clé privée
  ni secret de signature n'est présent dans l'application ni dans le dépôt. Une période de grâce hors
  ligne (défaut 30 jours après une première activation en ligne, configurable) évite toute
  interruption pour cause de réseau. L'activation en ligne (facultative) associe la licence à un
  nombre de postes.
- **Critère d'acceptation** : une licence falsifiée (un octet modifié) est refusée ; l'application
  fonctionne 30 jours sans réseau après activation ; la clé privée absente est confirmée par
  recherche dans les binaires livrés et l'historique Git.
- **Test** : T-38 **[AUTO]** — jeu de tests de vérification (± altérations), recherche de motifs de
  clé dans les artefacts de build.
- **Priorité** : V1 (structure), V2 (activation en ligne).

### RF-39 — Gratuit/payant explicite et réversible
- **Comportement attendu** : les fonctionnalités payantes sont visibles et étiquetées dans
  l'interface, jamais retirées silencieusement après coup ; l'utilisateur peut exporter toutes ses
  données (dictionnaires, réglages, historique) sans licence active ; la dégradation d'une version
  Pro vers Free conserve les réglages et ne supprime jamais l'historique.
- **Critère d'acceptation** : parcours complet testé — Pro → Free sans perte de données, export
  fonctionnel sans licence, aucune fonctionnalité déjà utilisée désactivée sans avertissement
  préalable à la mise à jour.
- **Test** : T-39 **[MANUEL]** — scénario de dégradation et de migration.
- **Priorité** : V2.

---

## 12. Exigences non fonctionnelles (résumé)

| ID | Exigence | Où c'est mesuré |
|---|---|---|
| RNF-01 | Ensemble installé ≤ 60 Mo **hors modèles** ; modèles téléchargés séparément avec taille annoncée | `docs/06`, §3 |
| RNF-02 | Mémoire au repos et pendant la transcription : mesurée et publiée par configuration matérielle | `docs/06`, §3 |
| RNF-03 | Latence fin de parole → texte inséré : mesurée en p50/p95 | `docs/06`, §4 |
| RNF-04 | WER français : mesuré sur corpus reproductible, publié avec le modèle et le matériel | `docs/06`, §5 |
| RNF-05 | Stabilité sur dictée longue (60 min) sans fuite mémoire | `docs/06`, §6 |
| RNF-06 | Compatibilité : matrice de 12 applications + 3 versions de Windows | `docs/06`, §7 |
| RNF-07 | Fonctionnement dégradé propre (pas de GPU, pas de réseau, Windows on ARM) | `docs/06`, §8 |
| RNF-08 | Aucune donnée dans les journaux ; aucune connexion non consentie | RF-34, RF-35, `docs/06`, §9 |

---

## 13. Matrice de priorité (résumé)

| Priorité | Exigences |
|---|---|
| **MVP (P1)** | RF-01, RF-02(a,b), RF-03, RF-04, RF-06, RF-07, RF-08, RF-09 (partiel), RF-12, RF-13, RF-14, RF-15, RF-16, RF-18, RF-20, RF-22, RF-28, RF-29, RF-30, RF-31, RF-34, RF-35 |
| **V1 (P2–P3)** | RF-02(c), RF-05, RF-10, RF-11, RF-17, RF-19, RF-21, RF-23, RF-24, RF-26 (niveaux 0-1), RF-32, RF-33, RF-36, RF-37, RF-38 |
| **V2 (P4+)** | RF-25, RF-26 (niveaux 2-3), RF-27, RF-39 |
