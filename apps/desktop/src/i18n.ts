/** i18n minimal de Voxli (RF-17 amendé, docs/11) : la langue de l'interface
 * (`uiLanguage`, `fr`|`en`) est indépendante de la langue d'écoute ASR
 * (`general.language`). Règle de repli : toute valeur inconnue retombe sur
 * `fr` — jamais d'écran abscons (RF-29 : messages ouverts, jamais muets).
 *
 * Mécanisme :
 * - `setUiLanguage()` normalise la langue et pose `<html lang>` ;
 * - `t("clé")` rend la chaîne courante, avec interpolation `{n}`-like
 *   (`t("clé", { n: 12 })`) ;
 * - `applyDom()` applique la langue aux éléments statiques :
 *   `[data-i18n]` (textContent), `[data-i18n-html]` (innerHTML — table
 *   de chaînes propre, JAMAIS de contenu utilisateur), `[data-i18n-title]`
 *   (attribut title) et `[data-i18n-placeholder]` (attribut placeholder).
 *
 * Convention : la clé elle-même est rendue si elle manque à la table — le
 * trou de traduction reste lisible plutôt que silencieux ; les clés sont
 * préfixées par écran (state., hk., audio., models., history., license.,
 * privacy., diag., rescue., pill., notice., key., engine.).
 */

export type UiLanguage = "fr" | "en";

/** [fr, en] — le français est la référence (distribution cible). */
const STRINGS: Record<string, [string, string]> = {
  // Squelette / mode simple-avancé -------------------------------------------
  "app.tagline": [
    "Dictée par la voix, 100 % locale. Vos mots ne quittent jamais cette machine — sauf si vous le demandez explicitement.",
    "Voice dictation, 100% local. Your words never leave this machine — unless you explicitly ask for it.",
  ],
  "mode.advanced": [
    "Mode avancé — tous les réglages",
    "Advanced mode — every setting",
  ],
  "mode.theme": ["Thème", "Theme"],
  "theme.system": ["celui du système", "follow the system"],
  "theme.light": ["clair", "light"],
  "theme.dark": ["sombre", "dark"],

  // Onboarding 1-clic ---------------------------------------------------------
  "onboarding.title": [
    "🚀 Bienvenue sur Voxli !",
    "🚀 Welcome to Voxli!",
  ],
  "onboarding.intro": [
    "Prêt pour une dictée vocale instantanée, fluide et 100 % locale ?",
    "Ready for instant, smooth, 100% local voice dictation?",
  ],
  "onboarding.desc": [
    "Pour démarrer immédiatement, installez le modèle Whisper compact en un clic (57 Mo, téléchargé une seule fois, tout tourne hors-ligne) :",
    "To start right away, install the compact Whisper model in one click (57 MB, downloaded once, everything runs offline):",
  ],
  "onboarding.download": [
    "⚡ Télécharger et activer Whisper (1 clic)",
    "⚡ Download and enable Whisper (1 click)",
  ],

  // État ----------------------------------------------------------------------
  "state.section": ["État", "Status"],
  "state.engineWord": ["moteur :", "engine:"],
  "audio.meterTitle": [
    "Niveau du micro — jamais envoyé, jamais enregistré",
    "Microphone level — never sent, never recorded",
  ],
  "state.idle": ["prête", "idle"],
  "state.recording": ["enregistrement…", "recording…"],
  "state.finalizing": ["finalisation…", "finalizing…"],
  "state.transcribing": ["transcription…", "transcribing…"],
  "state.post-processing": ["post-traitement…", "post-processing…"],
  "state.inserting": ["insertion…", "inserting…"],
  "state.recovery": ["incident — réessayer possible", "error — retry possible"],

  // Général -------------------------------------------------------------------
  "general.section": ["Général", "General"],
  "general.languageLabel": [
    "Langue de dictée (auto = la machine la détecte)",
    "Dictation language (auto = the app detects it)",
  ],
  "general.languageAuto": [
    "Détection automatique (recommandée)",
    "Automatic detection (recommended)",
  ],
  "general.uiLanguageLabel": ["Langue de l'interface", "Interface language"],
  "general.uiLanguageFr": ["français", "français"],
  "general.uiLanguageEn": ["English", "English"],
  "general.postLabel": ["Mise en forme du texte", "Text formatting"],
  "general.postNone": ["aucune — texte brut", "none — raw text"],
  "general.postRules": ["règles françaises (défaut)", "French rules (default)"],
  "general.save": ["Enregistrer les réglages", "Save settings"],
  "general.saved": ["enregistré — appliqué à chaud", "saved — applied live"],

  // Raccourcis ----------------------------------------------------------------
  "hk.section": [
    "Raccourcis & déclenchement",
    "Shortcuts & triggering",
  ],
  "hk.pushLabel": ["Dictée", "Dictation"],
  "hk.modeLabel": ["Mode de déclenchement", "Trigger mode"],
  "hk.hold": ["appui maintenu", "push and hold"],
  "hk.toggle": ["bascule (un appui = marche/arrêt)", "toggle (one press = on/off)"],
  "hk.doublePushLock": ["double appui = verrouillé", "double press = locked"],
  "hk.pasteLabel": ["Coller le dernier texte", "Paste the last text"],
  "hk.showLabel": ["Montrer le dernier texte", "Show the last text"],
  "hk.note": [
    "Appliqués immédiatement à l'enregistrement (RF-01) — si un raccourci est déjà pris par une autre application, un message le signale.",
    "Applied to the recording immediately (RF-01) — if another application already owns a shortcut, a message says so.",
  ],

  // Vocabulaire ----------------------------------------------------------------
  "vocab.section": [
    "Dictionnaire & Vocabulaire personnalisé",
    "Dictionary & custom vocabulary",
  ],
  "vocab.intro": [
    "Enseignez à Voxli vos acronymes, noms propres, entreprises ou jargon métier. Ces termes sont injectés dans Whisper pour une précision chirurgicale (Magic Formatting).",
    "Teach Voxli your acronyms, proper names, companies or business jargon. These terms are injected into Whisper for surgical precision (Magic Formatting).",
  ],
  "vocab.profileLabel": ["Profil de vocabulaire :", "Vocabulary profile:"],
  "vocab.profileDefault": [
    "Général (actif partout)",
    "General (active everywhere)",
  ],
  "vocab.profileCode": [
    "Profil « Code & Dev »",
    "“Code & Dev” profile",
  ],
  "vocab.profileEmail": [
    "Profil « Courriels & Messages »",
    "“Emails & Messages” profile",
  ],
  "vocab.wordPlaceholder": [
    "Terme exact (ex : Voxli, CEFIR, SaaS)",
    "Exact term (e.g. Voxli, CEO, SaaS)",
  ],
  "vocab.variantsPlaceholder": [
    "Variantes orales (optionnel, ex : voxi, boxli)",
    "Spoken variants (optional, e.g. voxli, boxeee)",
  ],
  "vocab.add": ["+ Ajouter", "+ Add"],
  "vocab.empty": [
    "Aucun mot personnalisé dans ce profil. Ajoutez des noms propres, acronymes ou termes métier.",
    "No custom word in this profile yet. Add proper names, acronyms or business terms.",
  ],
  "vocab.deleteTitle": ["Supprimer ce terme", "Remove this term"],
  "vocab.exportCsv": ["Exporter en CSV", "Export as CSV"],
  "vocab.importCsv": ["Importer un fichier CSV", "Import a CSV file"],

  // Audio ----------------------------------------------------------------------
  "audio.section": ["Réglages audio", "Audio settings"],
  "audio.deviceLabel": ["Microphone", "Microphone"],
  "audio.deviceDefault": [
    "Micro par défaut du système",
    "System default microphone",
  ],
  "audio.defaultSuffix": ["(défaut)", "(default)"],
  "audio.refresh": ["Rafraîchir la liste", "Refresh list"],
  "audio.preRollLabel": [
    "Anticipation avant l'appui (pré-roll, ms)",
    "Head start before the press (pre-roll, ms)",
  ],
  "audio.maxRecLabel": [
    "Durée maximale d'une prise (s)",
    "Maximum recording length (s)",
  ],
  "audio.vadLabel": ["Détection de voix", "Voice detection"],
  "audio.vadLow": [
    "permissive — capte la parole lointaine",
    "permissive — catches far-away speech",
  ],
  "audio.vadBalanced": ["équilibrée (défaut)", "balanced (default)"],
  "audio.vadHigh": [
    "stricte — ignore le bruit",
    "strict — ignores noise",
  ],

  // Modèles --------------------------------------------------------------------
  "models.section": [
    "Voix (moteur de reconnaissance)",
    "Voice (recognition engine)",
  ],
  "models.intro": [
    "Le téléchargement est <b>votre action explicite</b>, ici, et la <b>seule</b> connexion réseau de Voxli depuis cet écran. Chaque fichier est vérifié après réception ; la preuve reste dans un journal local.",
    "Downloading is <b>your explicit action</b>, here, and the <b>only</b> network connection Voxli opens from this screen. Every file is verified after reception; the proof stays in a local log.",
  ],
  "models.colModel": ["Modèle", "Model"],
  "models.colState": ["État", "Status"],
  "models.colCheck": ["Contrôle", "Integrity"],
  "models.colActions": ["Actions", "Actions"],
  "models.installed": ["installé", "installed"],
  "models.notInstalled": ["non installé", "not installed"],
  "models.remove": ["retirer", "remove"],
  "models.download": ["télécharger", "download"],
  "models.downloading": ["téléchargement…", "downloading…"],
  "models.downloadingName": ["Téléchargement de {name}…", "Downloading {name}…"],
  "models.progress": [
    "{name} — {n} Mo / {n2} Mo ({p} %)",
    "{name} — {n} MB / {n2} MB ({p}%)",
  ],
  "models.progressUnknown": [
    "{name} — {n} Mo (taille inconnue)",
    "{name} — {n} MB (size unknown)",
  ],
  "models.cancel": ["Annuler", "Cancel"],
  "models.cancelled": [
    "Téléchargement annulé — reprise possible à tout moment.",
    "Download cancelled — resumable at any time.",
  ],
  "models.busy": [
    "Un téléchargement est déjà en cours.",
    "A download is already running.",
  ],
  "models.mbDownloaded": ["{n} Mo téléchargés", "{n} MB downloaded"],
  "models.sizeMb": ["— {n} Mo env.", "— about {n} MB"],
  "models.verifiedBadge": [
    "correspond à la référence de confiance publiée (docs/11 §3)",
    "matches the published trusted reference (docs/11 §3)",
  ],

  // Moteurs (sélection, écran Voix) --------------------------------------------
  "engines.choose": [
    "Moteur de dictée — le changement s'applique immédiatement :",
    "Dictation engine — the change applies immediately:",
  ],
  "engines.auto": ["Automatique (recommandé)", "Automatic (recommended)"],
  "engines.whisper": ["whisper.cpp — local", "whisper.cpp — local"],
  "engines.nemo": [
    "Nemotron 3.5 streaming — texte en direct",
    "Nemotron 3.5 streaming — real-time",
  ],
  "engines.parakeet": ["Parakeet TDT — local", "Parakeet TDT — local"],
  "engines.hintModel": [
    "indisponible — installez son modèle ci-dessous",
    "unavailable — install its model below",
  ],
  "engines.hintSdk": [
    "indisponible — SDK non inclus dans ce build",
    "unavailable — SDK not included in this build",
  ],
  "engines.effective": ["Moteur effectif : {engine}", "Active engine: {engine}"],

  // Profils --------------------------------------------------------------------
  "profiles.section": ["Profils & dictionnaires", "Profiles & dictionaries"],
  "profiles.lockNotice": [
    "Profils multiples : fonctionnalité Pro. Sans licence, seul le profil par défaut s'applique — vos liaisons restent enregistrées ici.",
    "Multiple profiles: a Pro feature. Without a licence only the default profile applies — your bindings stay recorded here.",
  ],
  "profiles.codeAppsLabel": [
    "Profil « code » — applications associées (séparées par des virgules)",
    "“Code” profile — associated apps (comma-separated)",
  ],
  "profiles.emailAppsLabel": [
    "Profil « courriel » — applications associées",
    "“Email” profile — associated apps",
  ],
  "profiles.dictCodeSummary": [
    "Dictionnaire « code » (une entrée par ligne : cible|variante1|variante2)",
    "“Code” dictionary (one entry per line: target|variant1|variant2)",
  ],
  "profiles.dictEmailSummary": ["Dictionnaire « courriel »", "“Email” dictionary"],
  "profiles.exportFile": ["Exporter en fichier", "Export to file"],
  "profiles.reimportFile": [
    "Réimporter depuis le fichier choisi",
    "Re-import from the chosen file",
  ],
  "profiles.importCsvCfg": [
    "Importer ce CSV dans la configuration",
    "Import this CSV into the configuration",
  ],
  "profiles.note": [
    "Le dictionnaire remplace vos variantes (métallurgie → « métallurjie » redevient <i>métallurgie</i>) même en mode « texte brut » : c'est votre correction, pas une réécriture du produit.",
    "The dictionary replaces your variants (metallurgy → “metallurjie” becomes <i>metallurgy</i> again) even in “raw text” mode: it is your correction, not a product rewrite.",
  ],
  "dict.imported": [
    "Dictionnaire importé : {n} entrée(s).",
    "Dictionary imported: {n} entry(ies).",
  ],

  // Historique -----------------------------------------------------------------
  "history.section": ["Historique", "History"],
  "history.keepLabel": [
    "Conserver les transcriptions",
    "Keep transcripts",
  ],
  "history.no": [
    "non — rien n'est conservé (défaut navigateur)",
    "no — nothing is kept (browser default)",
  ],
  "history.yes": [
    "oui — je le veux, supprimables à tout instant",
    "yes — I want it, deletable at any time",
  ],
  "history.whereLabel": ["Où conserver", "Where to keep"],
  "history.memory": [
    "mémoire vive seule (disparaît à l'extinction)",
    "memory only (gone at shutdown)",
  ],
  "history.disk": [
    "disque chiffré (clé Windows, DPAPI)",
    "encrypted disk (Windows key, DPAPI)",
  ],
  "history.retentionLabel": ["Rétention (jours)", "Retention (days)"],
  "history.purge": [
    "Tout supprimer maintenant",
    "Delete everything now",
  ],
  "history.purged": ["historique purgé", "history purged"],
  "history.colDate": ["Date", "Date"],
  "history.colEngine": ["Moteur", "Engine"],
  "history.colText": ["Texte", "Text"],
  "history.delete": ["supprimer", "delete"],
  "history.empty": ["aucune entrée", "no entry"],

  // Licence --------------------------------------------------------------------
  "license.section": ["Licence", "Licence"],
  "license.active": [
    "Licence active — titulaire : {name}.",
    "Active licence — holder: {name}.",
  ],
  "license.grace": [
    "Licence expirée — période de grâce hors ligne : encore {days} jour(s) de fonctionnalités Pro, puis retour à l'édition gratuite.",
    "Licence expired — offline grace period: {days} day(s) of Pro features left, then back to the free edition.",
  ],
  "license.free": [
    "Édition gratuite — tout le local fonctionne ; les fonctions Pro (historique chiffré, profils multiples) restent masquées.",
    "Free edition — everything local works; Pro features (encrypted history, multiple profiles) stay hidden.",
  ],
  "license.noFeatures": [
    "aucune fonctionnalité Pro",
    "no Pro feature",
  ],
  "license.importSummary": [
    "Importer un code de licence",
    "Import a licence code",
  ],
  "license.importNote": [
    "La vérification est locale et hors ligne (signature Ed25519). Aucun envoi.",
    "Verification is local and offline (Ed25519 signature). Nothing is sent.",
  ],
  "license.importAction": [
    "Vérifier et importer",
    "Verify and import",
  ],
  "license.imported": [
    "licence importée et vérifiée.",
    "licence imported and verified.",
  ],

  // Vie privée -----------------------------------------------------------------
  "privacy.simpleNote": [
    "Totalement local : rien ne quitte cette machine sans votre accord explicite. Les détails (consentements, lecture du contexte) sont dans le mode avancé.",
    "Fully local: nothing leaves this machine without your explicit consent. Details (consents, context reading) live in advanced mode.",
  ],
  "privacy.section": ["Vie privée", "Privacy"],
  "privacy.strictLabel": ["Mode local strict", "Strict local mode"],
  "privacy.strictOn": [
    "actif — rien ne quitte cette machine",
    "on — nothing leaves this machine",
  ],
  "privacy.strictOff": [
    "désactivé — je débloque sur consentement",
    "off — I unlock it on consent",
  ],
  "privacy.consentAsrLabel": [
    "Consentement : transcription distante (BYOK, P4)",
    "Consent: remote transcription (BYOK, P4)",
  ],
  "privacy.never": ["jamais (défaut)", "never (default)"],
  "privacy.grantedApiKey": [
    "accord — uniquement si j'ai saisi une clé d'API",
    "granted — only if I entered an API key",
  ],
  "privacy.consentPostLabel": [
    "Consentement : post-traitement distant",
    "Consent: remote post-processing",
  ],
  "privacy.granted": ["accord", "granted"],
  "privacy.contextLabel": ["Lecture du contexte (RF-37)", "Context reading (RF-37)"],
  "privacy.contextNone": [
    "aucune lecture (défaut)",
    "no reading (default)",
  ],
  "privacy.contextAppTitle": [
    "application + titre (pas le contenu)",
    "app + title (not the content)",
  ],
  "privacy.contextSelection": [
    "application + titre + sélection",
    "app + title + selection",
  ],
  "privacy.contextPreview": [
    "Voir le contexte actuel",
    "Preview current context",
  ],
  "privacy.contextEmpty": ["aucun contexte lu", "no context read"],
  "privacy.note": [
    "Aucun service distant n'est activé tant que ces deux interrupteurs restent sur « jamais » — même si un moteur distant est compilé.",
    "No remote service is active while both switches stay on “never” — even if a remote engine is compiled.",
  ],

  // Diagnostic -----------------------------------------------------------------
  "diag.section": ["Diagnostic & données", "Diagnostics & data"],
  "diag.exportReport": [
    "Exporter le rapport de diagnostic",
    "Export the diagnostic report",
  ],
  "diag.exportData": [
    "Exporter mes données (configuration + dictionnaires)",
    "Export my data (configuration + dictionaries)",
  ],
  "diag.downloadLink": ["Télécharger le fichier", "Download the file"],
  "diag.note": [
    "Le rapport ne contient, par conception, ni caractère dicté, ni audio, ni contenu de presse-papiers. Relisez-le avant tout envoi.",
    "By design the report contains no dictated character, no audio, no clipboard content. Read it back before sending anything.",
  ],

  // Pied de page ---------------------------------------------------------------
  "footer.text": [
    "Voxli — 100 % local. Réglages avancés accessibles dans <code>%APPDATA%\\Voxli\\config.json</code>.",
    "Voxli — 100% local. Advanced settings live in <code>%APPDATA%\\Voxli\\config.json</code>.",
  ],

  // Notifications dynamiques ---------------------------------------------------
  "notice.modelStarted": [
    "Téléchargement du modèle Whisper démarré (57 Mo)...",
    "Whisper model download started (57 MB)...",
  ],
  "notice.modelInterrupted": [
    "Téléchargement interrompu ({reason}). Vous pouvez recommencer : il reprendra là où il s'était arrêté.",
    "Download interrupted ({reason}). You can try again: it will resume where it stopped.",
  ],
  "notice.modelOk": [
    "Modèle installé — pris en compte immédiatement.",
    "Model installed — picked up immediately.",
  ],
  "notice.networkError": ["erreur réseau", "network error"],

  // Fenêtre de secours ---------------------------------------------------------
  "rescue.title": ["Texte de secours", "Rescue text"],
  "rescue.intro": [
    "Ce texte vient d'être dicté : il n'a pas pu être inséré dans l'application cible. Il reste sur votre machine, dans le tampon interne de Voxli — rien d'autre n'en a gardé trace.",
    "This text was just dictated: it could not be inserted into the target application. It stays on your machine, in Voxli's internal buffer — nothing else kept a trace of it.",
  ],
  "rescue.retry": ["Réessayer", "Try again"],
  "rescue.pasteAgain": ["Coller à nouveau", "Paste again"],
  "rescue.selectAll": ["Sélectionner tout", "Select all"],
  "rescue.close": ["Fermer", "Close"],
  "rescue.emptyHint": [
    "Aucun texte en attente. Si une transcription vient d'échouer, l'audio est conservé : appuyez sur « Réessayer ».",
    "No text waiting. If a transcription just failed, the audio is kept: press “Try again”.",
  ],

  // Pastille -------------------------------------------------------------------
  "pill.legend": ["Dictée", "Dictation"],

  // Noms de touches (légende de survol) ---------------------------------------
  "key.Space": ["Espace", "Space"],
  "key.Control": ["Ctrl", "Ctrl"],
  "key.Ctrl": ["Ctrl", "Ctrl"],
  "key.Shift": ["Maj", "Shift"],
  "key.Alt": ["Alt", "Alt"],
  "key.AltGr": ["Alt Gr", "AltGr"],
  "key.Meta": ["Win", "Win"],
  "key.Win": ["Win", "Win"],
  "key.Enter": ["Entrée", "Enter"],
  "key.Escape": ["Échap", "Esc"],

  // Libellés moteur ------------------------------------------------------------
  "engine.fake": ["moteur factice (démo)", "dummy engine (demo)"],
  "engine.whisper.cpp": [
    "whisper.cpp — 100 % local",
    "whisper.cpp — 100% local",
  ],
  "engine.parakeet": [
    "Parakeet — 100 % local",
    "Parakeet — 100% local",
  ],
  "engine.parakeet-onnx": [
    "Parakeet — 100 % local",
    "Parakeet — 100% local",
  ],
  "engine.nemo": [
    "Nemotron 3.5 — 100 % local, texte en direct",
    "Nemotron 3.5 — 100% local, live text",
  ],
};

let current: UiLanguage = "fr";

/** Normalise une valeur de configuration : toute autre chose que « en »
 * retombe sur « fr » (repli lisible, jamais de trou). */
export function setUiLanguage(raw: string): UiLanguage {
  current = raw === "en" ? "en" : "fr";
  document.documentElement.lang = current;
  return current;
}

export function uiLanguage(): UiLanguage {
  return current;
}

/** Chaîne courante d'une clé, avec interpolation optionnelle {n}-like. */
export function t(key: string, args?: Record<string, string | number>): string {
  const entry = STRINGS[key];
  let s = entry === undefined ? key : entry[current === "en" ? 1 : 0];
  if (args !== undefined) {
    for (const [name, value] of Object.entries(args)) {
      s = s.split(`{${name}}`).join(String(value));
    }
  }
  return s;
}

/** Applique la langue courante aux éléments statiques de la fenêtre. */
export function applyDom(): void {
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n]"))) {
    const key = el.dataset.i18n;
    if (key !== undefined && key !== "") el.textContent = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-html]"))) {
    const key = el.dataset.i18nHtml;
    // innerHTML ici ne porte JAMAIS de contenu utilisateur : les valeurs
    // sont les chaînes statiques ci-dessus (seul le balisage de présentation).
    if (key !== undefined && key !== "") el.innerHTML = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-title]"))) {
    const key = el.dataset.i18nTitle;
    if (key !== undefined && key !== "") el.title = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-placeholder]"))) {
    const key = el.dataset.i18nPlaceholder;
    if (key !== undefined && key !== "") (el as HTMLInputElement).placeholder = t(key);
  }
}

/** Capsule : définir la langue puis peindre le document statique. */
export function applyUiLanguage(raw: string): void {
  setUiLanguage(raw);
  applyDom();
}
