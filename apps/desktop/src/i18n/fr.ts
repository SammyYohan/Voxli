/** i18n Voxli v2 — référence FRANÇAISE (distribution cible). Traduction
 * des clés de la référence anglaise ; une clé manquante retombe sur `en`
 * puis `fr` (jamais d'écran abscons, RF-29).
 */
export const dict: Record<string, string> = {
  // Squelette / mode simple-avancé
  "app.tagline": "Dictée par la voix, 100 % locale. Vos mots ne quittent jamais cette machine — sauf si vous le demandez explicitement.",
  "mode.advanced": "Mode avancé — tous les réglages",
  "mode.theme": "Thème",
  "theme.system": "celui du système",
  "theme.light": "clair",
  "theme.dark": "sombre",

  // Onboarding 1-clic
  "onboarding.title": "🚀 Bienvenue sur Voxli !",
  "onboarding.intro": "Prêt pour une dictée vocale instantanée, fluide et 100 % locale ?",
  "onboarding.desc": "Pour démarrer immédiatement, installez le modèle Whisper compact en un clic (57 Mo, téléchargé une seule fois, tout tourne hors-ligne) :",
  "onboarding.download": "⚡ Télécharger et activer Whisper (1 clic)",

  // État
  "state.section": "État",
  "state.engineWord": "moteur :",
  "audio.meterTitle": "Niveau du micro — jamais envoyé, jamais enregistré",
  "state.idle": "prête",
  "state.recording": "enregistrement…",
  "state.finalizing": "finalisation…",
  "state.transcribing": "transcription…",
  "state.post-processing": "post-traitement…",
  "state.inserting": "insertion…",
  "state.recovery": "incident — réessayer possible",

  // Général
  "general.section": "Général",
  "general.languageLabel": "Langue de dictée (auto = la machine la détecte)",
  "general.languageAuto": "Détection automatique (recommandée)",
  "general.uiLanguageLabel": "Langue de l'interface",
  "general.uiSystem": "Langue du système (détection automatique)",
  "general.postLabel": "Mise en forme du texte",
  "general.postNone": "aucune — texte brut",
  "general.postRules": "règles françaises (défaut)",
  "general.save": "Enregistrer les réglages",
  "general.saved": "enregistré — appliqué à chaud",

  // Raccourcis
  "hk.section": "Raccourcis & déclenchement",
  "hk.pushLabel": "Dictée",
  "hk.modeLabel": "Mode de déclenchement",
  "hk.hold": "appui maintenu",
  "hk.toggle": "bascule (un appui = marche/arrêt)",
  "hk.doublePushLock": "double appui = verrouillé",
  "hk.pasteLabel": "Coller le dernier texte",
  "hk.showLabel": "Montrer le dernier texte",
  "hk.note": "Appliqués immédiatement à l'enregistrement (RF-01) — si un raccourci est déjà pris par une autre application, un message le signale.",

  // Vocabulaire
  "vocab.section": "Dictionnaire & Vocabulaire personnalisé",
  "vocab.intro": "Enseignez à Voxli vos acronymes, noms propres, entreprises ou jargon métier. Ces termes sont injectés dans Whisper pour une précision chirurgicale (Magic Formatting).",
  "vocab.profileLabel": "Profil de vocabulaire :",
  "vocab.profileDefault": "Général (actif partout)",
  "vocab.profileCode": "Profil « Code & Dev »",
  "vocab.profileEmail": "Profil « Courriels & Messages »",
  "vocab.wordPlaceholder": "Terme exact (ex : Voxli, CEFIR, SaaS)",
  "vocab.variantsPlaceholder": "Variantes orales (optionnel, ex : voxi, boxli)",
  "vocab.add": "+ Ajouter",
  "vocab.empty": "Aucun mot personnalisé dans ce profil. Ajoutez des noms propres, acronymes ou termes métier.",
  "vocab.deleteTitle": "Supprimer ce terme",
  "vocab.exportCsv": "Exporter en CSV",
  "vocab.importCsv": "Importer un fichier CSV",

  // Audio
  "audio.section": "Réglages audio",
  "audio.deviceLabel": "Microphone",
  "audio.deviceDefault": "Micro par défaut du système",
  "audio.defaultSuffix": "(défaut)",
  "audio.refresh": "Rafraîchir la liste",
  "audio.preRollLabel": "Anticipation avant l'appui (pré-roll, ms)",
  "audio.maxRecLabel": "Durée maximale d'une prise (s)",
  "audio.vadLabel": "Détection de voix",
  "audio.vadLow": "permissive — capte la parole lointaine",
  "audio.vadBalanced": "équilibrée (défaut)",
  "audio.vadHigh": "stricte — ignore le bruit",

  // Modèles
  "models.section": "Voix (moteur de reconnaissance)",
  "models.intro": "Le téléchargement est <b>votre action explicite</b>, ici, et la <b>seule</b> connexion réseau de Voxli depuis cet écran. Chaque fichier est vérifié après réception ; la preuve reste dans un journal local.",
  "models.colModel": "Modèle",
  "models.colState": "État",
  "models.colCheck": "Contrôle",
  "models.colActions": "Actions",
  "models.installed": "installé",
  "models.notInstalled": "non installé",
  "models.remove": "retirer",
  "models.download": "télécharger",
  "models.downloading": "téléchargement…",
  "models.downloadingName": "Téléchargement de {name}…",
  "models.progress": "{name} — {n} Mo / {n2} Mo ({p} %)",
  "models.progressUnknown": "{name} — {n} Mo (taille inconnue)",
  "models.cancel": "Annuler",
  "models.cancelled": "Téléchargement annulé — reprise possible à tout moment.",
  "models.busy": "Un téléchargement est déjà en cours.",
  "models.mbDownloaded": "{n} Mo téléchargés",
  "models.sizeMb": "— {n} Mo env.",
  "models.verifiedBadge": "correspond à la référence de confiance publiée (docs/11 §3)",

  // Moteurs (sélection, écran Voix)
  "engines.choose": "Moteur de dictée — le changement s'applique immédiatement :",
  "engines.auto": "Automatique (recommandé)",
  "engines.whisper": "whisper.cpp — local",
  "engines.nemo": "Nemotron 3.5 streaming — texte en direct",
  "engines.parakeet": "Parakeet TDT — local",
  "engines.hintModel": "indisponible — installez son modèle ci-dessous",
  "engines.hintSdk": "indisponible — SDK non inclus dans ce build",
  "engines.effective": "Moteur effectif : {engine}",

  // Profils
  "profiles.section": "Profils & dictionnaires",
  "profiles.lockNotice": "Profils multiples : fonctionnalité Pro. Sans licence, seul le profil par défaut s'applique — vos liaisons restent enregistrées ici.",
  "profiles.codeAppsLabel": "Profil « code » — applications associées (séparées par des virgules)",
  "profiles.emailAppsLabel": "Profil « courriel » — applications associées",
  "profiles.dictCodeSummary": "Dictionnaire « code » (une entrée par ligne : cible|variante1|variante2)",
  "profiles.dictEmailSummary": "Dictionnaire « courriel »",
  "profiles.exportFile": "Exporter en fichier",
  "profiles.reimportFile": "Réimporter depuis le fichier choisi",
  "profiles.importCsvCfg": "Importer ce CSV dans la configuration",
  "profiles.note": "Le dictionnaire remplace vos variantes (métallurgie → « métallurjie » redevient <i>métallurgie</i>) même en mode « texte brut » : c'est votre correction, pas une réécriture du produit.",
  "dict.imported": "Dictionnaire importé : {n} entrée(s).",

  // Historique
  "history.section": "Historique",
  "history.keepLabel": "Conserver les transcriptions",
  "history.no": "non — rien n'est conservé (défaut navigateur)",
  "history.yes": "oui — je le veux, supprimables à tout instant",
  "history.whereLabel": "Où conserver",
  "history.memory": "mémoire vive seule (disparaît à l'extinction)",
  "history.disk": "disque chiffré (clé Windows, DPAPI)",
  "history.retentionLabel": "Rétention (jours)",
  "history.purge": "Tout supprimer maintenant",
  "history.purged": "historique purgé",
  "history.colDate": "Date",
  "history.colEngine": "Moteur",
  "history.colText": "Texte",
  "history.delete": "supprimer",
  "history.empty": "aucune entrée",

  // Licence
  "license.section": "Licence",
  "license.active": "Licence active — titulaire : {name}.",
  "license.grace": "Licence expirée — période de grâce hors ligne : encore {days} jour(s) de fonctionnalités Pro, puis retour à l'édition gratuite.",
  "license.free": "Édition gratuite — tout le local fonctionne ; les fonctions Pro (historique chiffré, profils multiples) restent masquées.",
  "license.noFeatures": "aucune fonctionnalité Pro",
  "license.importSummary": "Importer un code de licence",
  "license.importNote": "La vérification est locale et hors ligne (signature Ed25519). Aucun envoi.",
  "license.importAction": "Vérifier et importer",
  "license.imported": "licence importée et vérifiée.",

  // Vie privée
  "privacy.simpleNote": "Totalement local : rien ne quitte cette machine sans votre accord explicite. Les détails (consentements, lecture du contexte) sont dans le mode avancé.",
  "privacy.section": "Vie privée",
  "privacy.strictLabel": "Mode local strict",
  "privacy.strictOn": "actif — rien ne quitte cette machine",
  "privacy.strictOff": "désactivé — je débloque sur consentement",
  "privacy.consentAsrLabel": "Consentement : transcription distante (BYOK, P4)",
  "privacy.never": "jamais (défaut)",
  "privacy.grantedApiKey": "accord — uniquement si j'ai saisi une clé d'API",
  "privacy.consentPostLabel": "Consentement : post-traitement distant",
  "privacy.granted": "accord",
  "privacy.contextLabel": "Lecture du contexte (RF-37)",
  "privacy.contextNone": "aucune lecture (défaut)",
  "privacy.contextAppTitle": "application + titre (pas le contenu)",
  "privacy.contextSelection": "application + titre + sélection",
  "privacy.contextPreview": "Voir le contexte actuel",
  "privacy.contextEmpty": "aucun contexte lu",
  "privacy.note": "Aucun service distant n'est activé tant que ces deux interrupteurs restent sur « jamais » — même si un moteur distant est compilé.",

  // Diagnostic
  "diag.section": "Diagnostic & données",
  "diag.exportReport": "Exporter le rapport de diagnostic",
  "diag.exportData": "Exporter mes données (configuration + dictionnaires)",
  "diag.downloadLink": "Télécharger le fichier",
  "diag.note": "Le rapport ne contient, par conception, ni caractère dicté, ni audio, ni contenu de presse-papiers. Relisez-le avant tout envoi.",

  // Pied de page
  "footer.text": "Voxli — 100 % local. Réglages avancés accessibles dans <code>%APPDATA%\\Voxli\\config.json</code>.",

  // Dons (visibles mais discrets : pied de page permanent + encart périodique)
    "donate.bannerTitle": "Voxli est gratuit, local et sans publicité. Vous pouvez soutenir son développement :",
  "donate.kofi": "Offrir un café sur Ko-fi",
  "donate.paypal": "Don via PayPal.me",
  "donate.later": "Peut-être plus tard",
  "donate.forever": "Ne plus me le proposer",

  // Notifications dynamiques
  "notice.modelStarted": "Téléchargement du modèle Whisper démarré (57 Mo)...",
  "notice.modelInterrupted": "Téléchargement interrompu ({reason}). Vous pouvez recommencer : il reprendra là où il s'était arrêté.",
  "notice.modelOk": "Modèle installé — pris en compte immédiatement.",
  "notice.networkError": "erreur réseau",

  // Fenêtre de secours
  "rescue.title": "Texte de secours",
  "rescue.intro": "Ce texte vient d'être dicté : il n'a pas pu être inséré dans l'application cible. Il reste sur votre machine, dans le tampon interne de Voxli — rien d'autre n'en a gardé trace.",
  "rescue.retry": "Réessayer",
  "rescue.pasteAgain": "Coller à nouveau",
  "rescue.selectAll": "Sélectionner tout",
  "rescue.close": "Fermer",
  "rescue.emptyHint": "Aucun texte en attente. Si une transcription vient d'échouer, l'audio est conservé : appuyez sur « Réessayer ».",

  // Pastille
  "pill.legend": "Dictée",

  // Noms de touches (légende de survol)
  "key.Space": "Espace",
  "key.Control": "Ctrl",
  "key.Ctrl": "Ctrl",
  "key.Shift": "Maj",
  "key.Alt": "Alt",
  "key.AltGr": "Alt Gr",
  "key.Meta": "Win",
  "key.Win": "Win",
  "key.Enter": "Entrée",
  "key.Escape": "Échap",

  // Libellés moteur
  "engine.fake": "moteur factice (démo)",
  "engine.whisper.cpp": "whisper.cpp — 100 % local",
  "engine.parakeet": "Parakeet — 100 % local",
  "engine.parakeet-onnx": "Parakeet — 100 % local",
  "engine.nemo": "Nemotron 3.5 — 100 % local, texte en direct",
  "engines.calcLabel": "Calcul des transcriptions",
  "engines.calcAuto": "Automatique — GPU si plus rapide (recommandé)",
  "engines.calcGpu": "GPU — Vulkan/CUDA",
  "engines.calcCpu": "CPU — toujours disponible",
  "engines.calcNote": "« Auto » choisit le GPU quand un accélérateur est disponible et retombe sur le CPU sinon ; le journal consigne le choix et la latence réelle.",
  "engines.calcDuo": "Duo — partiels sur GPU, final sur CPU",
  "hk.captureHint": "Cliquez dans un champ puis tapez directement les touches du nouveau raccourci (Échap annule). Libres = validées aussitôt ; déjà prises = « occupé ».",
  "hk.conflict": "Raccourci déjà pris : il est utilisé par une autre action.",
  "hk.needModifier": "Incluez au moins un modificateur (Ctrl, Alt, Shift ou Win).",
  "hk.unsupported": "Touche non gérée ici : lettre, chiffre, F1–F24, Espace, Tab, Entrée ou `.",
  "models.connect": "Utiliser",
};
