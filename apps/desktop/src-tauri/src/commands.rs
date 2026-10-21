//! Commandes IPC — surface webview ⇄ noyau (docs/05 §3).
//!
//! Principe : le webview **observe** et **configure**. Il ne voit jamais
//! l'audio ; il ne déclenche aucune insertion de texte arbitraire.
//!
//! Commandes (table IPC, mise à jour docs/05 §3) :
//! - P0 : `configGet` / `configSet` / `stateGet` / `rescueLast` /
//!   `rescuePasteAgain` / `rescueRetry` / `rescueClose` / `cancelDictation` /
//!   `diagnosticsExport` ;
//! - P1 : `devicesList` (RF-08), `modelsList` / `modelsDownload` /
//!   `modelsDownloadCancel` / `modelsRemove` (RF-20), `historyQuery` /
//!   `historyDelete` / `historyPurge` (RF-33), `licenseImport` /
//!   `licenseStatus` (RF-38), `dataExport`, `dictionariesImport` /
//!   `dictionariesExport` (RF-23), `dictionaryPreview` (RF-37) ;
//! - événements additionnels documentés dans la table : `notice://notice`,
//!   `meter://level`, `models://progress`, `state://changed` (worker.rs).
//!
//! Garde T-34 : aucune commande ne renvoie de texte dicté, sauf les deux
//! exceptions documentées (fenêtre de secours RF-31, export de données
//! chiffré RF-33 — c'est une action explicite de l'utilisateur).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Wry};

use voxli_config::Config;
use voxli_diagnostics::{Event, Logger};
use voxli_license::Entitlement as LicenseEntitlement;

use crate::strings;
use crate::worker::WorkerMsg;

/// État partagé : thread principal ⇄ webview ⇄ thread pipeline. Tout champ est
/// `Send + Sync` ; les accès passent par `Mutex` avec récupération du poison
/// (jamais de panique sur un verrou empoisonné).
pub struct UiShared {
    /// File d'entrée du thread pipeline (événements de dictée).
    pub tx: Sender<WorkerMsg>,
    /// Configuration durablement en mémoire ; le disque en est le miroir.
    pub config: Mutex<Config>,
    pub config_path: PathBuf,
    /// Journal partagé : thread pipeline + commandes.
    pub logger: Arc<Logger>,
    /// Tampon « texte de secours » affiché dans la fenêtre RF-31 ; exception
    /// documentée : miroir du tampon interne de l'orchestrateur (RF-22).
    pub last: Mutex<Option<String>>,
    /// Nom d'état courant (machine à états docs/04 §3), jamais de contenu.
    pub state: Mutex<String>,
    /// Identifiant du moteur chargé (métadonnée, RF-34).
    pub engine: Mutex<String>,
    /// Licence vérifiée (RF-38) : rien de durable côté mémoire sans la
    /// signature Ed25519. Rechargée au démarrage, mise à jour par import.
    pub license: Mutex<LicenseInfo>,
    /// Store d'historique (RF-33) : partagé entre le worker (écriture à la
    /// fin de session) et les commandes IPC (lecture/suppression).
    pub history: Mutex<Option<Box<dyn voxli_storage::HistoryStore>>>,
    /// Téléchargement de modèle en cours (un seul à la fois, RF-20).
    pub download_busy: Mutex<bool>,
    pub download_cancel: Arc<AtomicBool>,
}

/// État de licence vu par l'UI (RF-38/RF-39) — jamais des clés, jamais
/// aucun secret : titulaire + fonctions + expiration.
#[derive(Clone, Debug, Serialize)]
pub struct LicenseInfo {
    /// `free` | `active` | `grace` | `expired`.
    pub state: String,
    pub licensee: String,
    pub features: Vec<String>,
    pub expires: Option<String>,
    /// Date de péremption en secondes (affichage « jours restants »).
    pub expires_at_secs: Option<u64>,
}

impl LicenseInfo {
    pub fn free() -> Self {
        Self {
            state: "free".into(),
            licensee: "gratuit".into(),
            features: Vec::new(),
            expires: None,
            expires_at_secs: None,
        }
    }
}

/// Helper d'accès antipanique au contenu d'un mutex.
pub(crate) fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ------------------------------------------------------------ licence ----

/// Vérifie le fichier de licence (`license.txt`) contre la clé publique
/// embarquée ; période de grâce hors ligne 30 jours (RF-38/ADR-08).
pub fn license_from_file(path: &std::path::Path) -> LicenseInfo {
    let Some(key) = voxli_license::embedded_public_key() else {
        return LicenseInfo::free();
    };
    let Ok(blob) = std::fs::read_to_string(path) else {
        return LicenseInfo::free();
    };
    let grace = std::time::Duration::from_secs(30 * 86_400);
    match voxli_license::verify(
        &blob,
        &key,
        voxli_license::PRODUCT,
        std::time::SystemTime::now(),
        grace,
    ) {
        Ok(ent) => license_info_from(ent),
        Err(_) => LicenseInfo::free(),
    }
}

fn license_info_from(ent: LicenseEntitlement) -> LicenseInfo {
    use voxli_core::{Entitlement as _, Feature};
    let mut features = Vec::new();
    for f in [
        Feature::HistoryOnDisk,
        Feature::CloudPostProcessing,
        Feature::ExtraProfiles,
    ] {
        if ent.feature(f) {
            features.push(f.as_str().to_string());
        }
    }
    // `verify` accepte une licence périmée dans la fenêtre de grâce (RF-38) :
    // on le signale — « grace », pas « active ».
    let now = now_secs();
    let state = match ent.expires_at_secs() {
        Some(e) if e <= now => "grace".to_string(),
        _ => "active".to_string(),
    };
    LicenseInfo {
        state,
        licensee: ent.licensee().to_string(),
        features,
        expires: None,
        expires_at_secs: ent.expires_at_secs(),
    }
}

#[derive(Serialize)]
pub struct LicenseStatus {
    pub license: LicenseInfo,
    /// Chemin du fichier attendu (aide à l'import manuel).
    pub path: String,
}

#[tauri::command]
pub fn license_status(shared: State<'_, UiShared>) -> LicenseStatus {
    LicenseStatus {
        license: lock_or_recover(&shared.license).clone(),
        path: license_path().display().to_string(),
    }
}

fn license_path() -> PathBuf {
    Config::default_dir().join("license.txt")
}

/// Importe un blob de licence signé (texte complet, collé ou fichier).
/// La vérification est locale, hors ligne, Ed25519 (ADR-08) ; le blob est
/// persisté **seulement si valide**.
#[tauri::command]
pub fn license_import(shared: State<'_, UiShared>, blob: String) -> Result<LicenseInfo, String> {
    let Some(key) = voxli_license::embedded_public_key() else {
        return Err(
            "Aucune clé publique embarquée dans ce binaire — import impossible.".to_string(),
        );
    };
    let grace = std::time::Duration::from_secs(30 * 86_400);
    let ent = voxli_license::verify(
        &blob,
        &key,
        voxli_license::PRODUCT,
        std::time::SystemTime::now(),
        grace,
    )
    .map_err(|e| format!("Licence refusée : {e}"))?;
    let info = license_info_from(ent);
    std::fs::write(license_path(), blob.trim())
        .map_err(|e| format!("{} : {e}", strings::ERR_WRITE))?;
    *lock_or_recover(&shared.license) = info.clone();
    // Les droits prennent effet **immédiatement** : le worker relit la
    // licence à chaque reconstruction — on lui envoie le signal, sans
    // recharger le moteur (`engine_stale` le voit inchangé).
    let _ = shared.tx.send(WorkerMsg::ConfigChanged(Box::new(
        lock_or_recover(&shared.config).clone(),
    )));
    Ok(info)
}

// ------------------------------------------------------------ config ----

#[tauri::command]
pub fn config_get(shared: State<'_, UiShared>) -> Config {
    lock_or_recover(&shared.config).clone()
}

/// Écriture de configuration : validation complète côté noyau, confiance
/// nulle envers le webview. RF-01 : les raccourcis sont **re-registrés à
/// chaud** (échec d'enregistrement consigné au journal, jamais de panique) ;
/// le thread pipeline reçoit `ConfigChanged` et reconstruit son orchestrateur
/// (tampon RF-22 préservé) — la modification est live, sans redémarrage.
#[tauri::command]
pub fn config_set(
    app: AppHandle<Wry>,
    shared: State<'_, UiShared>,
    new_config: Config,
) -> Result<(), String> {
    let normalized = new_config.normalized();
    // Palette de raccourcis : validation syntaxique + conflits internes
    // (deux raccourcis identiques sont interdits).
    let combos = [
        normalized.hotkeys.push_to_talk.clone(),
        normalized.hotkeys.paste_last_transcript.clone(),
        normalized.hotkeys.show_last_transcript.clone(),
    ];
    if combos.iter().any(|c| c.trim().is_empty()) {
        return Err("Un raccourci ne peut pas être vide.".to_string());
    }
    for (i, a) in combos.iter().enumerate() {
        for b in combos.iter().skip(i + 1) {
            if a.eq_ignore_ascii_case(b) {
                return Err(
                    "Conflit de raccourcis : deux actions utilisent la même combinaison."
                        .to_string(),
                );
            }
        }
    }
    // P2 (audit) : le nom de modèle reste un NOM SIMPLE du dossier
    // `%APPDATA%\Voxli\models` — le webview ne peut pas faire lire au
    // moteur un fichier hors du dossier de données.
    if !normalized.general.model.is_empty()
        && normalized.general.model != "auto"
        && !is_safe_model_name(&normalized.general.model)
    {
        return Err(
            "Nom de modèle invalide : un nom de fichier simple est attendu (ex. ggml-base-q5_1.bin)."
                .to_string(),
        );
    }
    normalized
        .save(&shared.config_path)
        .map_err(|e| format!("{} : {e}", strings::ERR_CONFIG_SAVE))?;
    *lock_or_recover(&shared.config) = normalized.clone();

    // RF-01 : re-registration des raccourcis globaux côté thread principal
    // (les commandes tauri s'exécutent dessus — plugin global-shortcut OK).
    crate::re_register_shortcuts(&app, &normalized);

    // Le pipeline re-construit orchestrateur, moteur, capture, historique.
    let _ = shared
        .tx
        .send(WorkerMsg::ConfigChanged(Box::new(normalized)));
    Ok(())
}

// ------------------------------------------------------------- état ----

#[derive(Serialize)]
pub struct StateInfo {
    /// Nom d'état de la machine à états (docs/04 §3).
    pub state: String,
    /// Identifiant du moteur chargé (métadonnée RF-34).
    pub engine: String,
}

#[tauri::command]
pub fn state_get(shared: State<'_, UiShared>) -> StateInfo {
    StateInfo {
        state: lock_or_recover(&shared.state).clone(),
        engine: lock_or_recover(&shared.engine).clone(),
    }
}

// ------------------------------------------------- périphériques RF-08 ---

#[derive(Serialize)]
pub struct DeviceInfo {
    /// Identifiant cpal stable (persiste dans `audio.device`).
    pub id: String,
    pub name: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

#[tauri::command]
pub fn devices_list() -> Vec<DeviceInfo> {
    voxli_audio::list_input_devices()
        .into_iter()
        .map(|d| DeviceInfo {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        })
        .collect()
}

// ------------------------------------------------------- modèles RF-20 ---

#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    /// "whisper" | "parakeet" | "nemo".
    pub kind: String,
    /// Taille estimée (Mo) — indicative.
    #[serde(rename = "sizeMb")]
    pub size_mb: Option<u64>,
    /// Licence du modèle téléchargé — affichée AVANT toute action.
    pub license: String,
    pub installed: bool,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
    /// Vrai si l'empreinte du manifeste ÉGAL la référence de confiance du
    /// catalogue (P1-3) : jamais vrai tant que `expected_sha256` reste
    /// `None` (aucun digest publié → rien n'est prétendu vérifié).
    pub verified: bool,
}

/// Catalogue : la provenance et la licence sont visibles dans l'interface
/// avant tout téléchargement (attribution CC-BY-4.0 / OpenMDW,
/// THIRD-PARTY-LICENSES.md).
struct ModelEntry {
    id: &'static str,
    label: &'static str,
    /// "whisper" | "parakeet" | "nemo".
    kind: &'static str,
    /// Taille estimée (Mo) — indicative.
    size_mb: Option<u64>,
    /// Licence du modèle téléchargé — affichée AVANT toute action.
    license: &'static str,
    /// Empreinte SHA-256 hex (minuscule) issue d'une source de CONFIANCE :
    /// manifeste de release signé par le mainteneur, puis comparée
    /// post-téléchargement. **Rien n'est inventé ici** — tant qu'aucune
    /// révision humaine n'a apporté le digest (procédure docs/11-checklist-
    /// release.md §3), la valeur reste `None` : l'empreinte calculée est
    /// consignée seule, avec `verified: false` (P1-3, docs/08-audit).
    expected_sha256: Option<&'static str>,
}

const MODEL_CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: "ggml-base-q5_1.bin",
        label: "Whisper base (multilingue, quantisé — 57 Mo)",
        kind: "whisper",
        size_mb: Some(57),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "ggml-tiny-q8_0.bin",
        label: "Whisper tiny (ultra-léger, machines modestes — 43 Mo)",
        kind: "whisper",
        size_mb: Some(43),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "ggml-small-q5_1.bin",
        label: "Whisper small (multilingue, quantisé — 190 Mo)",
        kind: "whisper",
        size_mb: Some(190),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "ggml-medium-q5_0.bin",
        label: "Whisper medium (précision étoffée, multilingue — 539 Mo)",
        kind: "whisper",
        size_mb: Some(539),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "ggml-large-v3-turbo-q5_0.bin",
        label: "Whisper large-v3-turbo (précision maximale, GPU conseillé — 574 Mo)",
        kind: "whisper",
        size_mb: Some(574),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf",
        label: "Nemotron 3.5 continu (40 langues, texte en direct — 742 Mo)",
        kind: "nemo",
        size_mb: Some(742),
        license: "OpenMDW-1.1 — NVIDIA Nemotron Speech",
        expected_sha256: None,
    },
    ModelEntry {
        id: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
        label: "Parakeet TDT v3 0.6B (int8, 25 langues — 487 Mo)",
        kind: "parakeet",
        size_mb: Some(487),
        license: "CC-BY-4.0 — NVIDIA NeMo (attribution requise)",
        expected_sha256: None,
    },
];

fn hash_of_manifest(id: &str) -> Option<String> {
    let manifests = Config::default_dir().join("models").join("manifests.json");
    let raw = std::fs::read_to_string(manifests).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("entries")
        .and_then(|e| e.get(id))
        .and_then(|m| m.get("sha256"))
        .and_then(|h| h.as_str())
        .map(String::from)
}

#[tauri::command]
pub fn models_list() -> Vec<ModelInfo> {
    let models_dir = Config::default_dir().join("models");
    MODEL_CATALOG
        .iter()
        .map(|entry| {
            let id = entry.id;
            let kind = entry.kind;
            let path = match kind {
                "parakeet" => models_dir.join(id).join("tokens.txt"),
                _ => models_dir.join(id),
            };
            let installed = match kind {
                "parakeet" => {
                    let dir = models_dir.join(id);
                    dir.join("encoder.int8.onnx").is_file() || dir.join("encoder.onnx").is_file()
                }
                _ => path.is_file(),
            };
            // Taille : pour Parakeet, somme réelle des fichiers du dossier
            // (la taille d'un répertoire NTFS ne vaut rien) — P2, audit §3.
            let size_bytes = if installed && kind == "parakeet" {
                Some(dir_size_bytes(&models_dir.join(id)))
            } else if installed {
                std::fs::metadata(&path).ok().map(|m| m.len())
            } else {
                None
            };
            // Statut vérifié (P1-3) : comparaison effective au niveau de la
            // liste aussi — l'utilisateur voit la correspondance réelle.
            let digest = hash_of_manifest(id);
            let verified = match (&entry.expected_sha256, digest.as_deref()) {
                (Some(expected), Some(actual)) => expected.eq_ignore_ascii_case(actual),
                _ => false,
            };
            ModelInfo {
                id: id.to_string(),
                label: entry.label.to_string(),
                kind: kind.to_string(),
                size_mb: entry.size_mb,
                license: entry.license.to_string(),
                installed,
                size_bytes,
                sha256: digest,
                verified,
            }
        })
        .collect()
}

/// Somme récursive des tailles de fichiers d'un dossier (0 si inexistant).
fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size_bytes(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// État déclaré d'un moteur pour l'écran « Voix » : compilé dans CE build
/// (feature) et réellement utilisable sur CETTE machine (modèle présent
/// sur le disque). Aucune promesse sans vérification.
#[derive(Serialize)]
pub struct EngineInfo {
    /// Identifiant config — valeurs de `general.engine` (serde lowercase).
    pub id: String,
    /// La feature moteur est compilée dans ce build.
    pub compiled: bool,
    /// Compilé ET prêt à dicter ici (modèle installé) — sinon grisé à l'UI.
    pub usable: bool,
}

/// Résolution du modèle whisper quand la config dit « auto » (ou vide) :
/// le premier fichier du catalogue présent sur disque, dans l'ordre de
/// préférence (base — bon équilibre — puis small, medium, large-v3-turbo,
/// tiny), sinon le défaut du catalogue. Partagée par `engines_list` et
/// `worker::model_path` : UNE seule vérité, le choix « auto » ne dépend
/// jamais du compte qui l'interroge (l'incohérence du 21/10 — moteur
/// « chargé » et table « non installé » — vient des résolutions jumelles).
///
/// Garde-fou : un id EXPLICITE de la config n'est honoré QUE s'il est du
/// type whisper dans le catalogue. Un id nemo/parakeet (empoisonné jadis
/// par l'ancienne auto-connexion sans garde, code 2 au journal) n'est
/// JAMAIS confié au loader whisper — repli sur la résolution auto.
pub(crate) fn resolve_whisper_model(cfg: &Config, models_dir: &std::path::Path) -> String {
    let explicit = cfg.general.model.as_str();
    let explicit_whisper = !explicit.is_empty()
        && explicit != "auto"
        && MODEL_CATALOG
            .iter()
            .any(|e| e.id == explicit && e.kind == "whisper");
    if explicit_whisper {
        return explicit.to_string();
    }
    for id in [
        "ggml-base-q5_1.bin",
        "ggml-small-q5_1.bin",
        "ggml-medium-q5_0.bin",
        "ggml-large-v3-turbo-q5_0.bin",
        "ggml-tiny-q8_0.bin",
    ] {
        if models_dir.join(id).is_file() {
            return id.to_string();
        }
    }
    "ggml-base-q5_1.bin".to_string()
}

/// Liste des moteurs candidats. La sélection s'écrit via `config_set`
/// (`general.engine`) et le worker reconstruit son bundle en live
/// (« engine_stale », RF-20) — jamais de redémarrage requis.
#[tauri::command]
pub fn engines_list(shared: State<'_, UiShared>) -> Vec<EngineInfo> {
    let cfg = lock_or_recover(&shared.config).clone();
    let models_dir = Config::default_dir().join("models");

    // whisper.cpp — modèle du réglage utilisateur, ou le meilleur fichier
    // présent quand `model` vaut « auto » (MÊME résolution que le worker).
    let whisper_compiled = cfg!(feature = "real-asr");
    let whisper_model = resolve_whisper_model(&cfg, &models_dir);
    let whisper_usable = whisper_compiled && models_dir.join(&whisper_model).is_file();

    // Nemotron streaming — le SDK est compilé (feature `nemo-stream`) et
    // le GGUF doit être sur le disque.
    let nemo_compiled = cfg!(feature = "nemo-stream");
    let nemo_usable = nemo_compiled
        && models_dir
            .join("nemotron-3.5-asr-streaming-0.6b.q8_0.gguf")
            .is_file();

    // Parakeet (sherpa-onnx) — installation = encoder présent, même
    // critère que `models_list`.
    let parakeet_compiled = cfg!(feature = "parakeet-asr");
    let parakeet_usable = parakeet_compiled
        && MODEL_CATALOG.iter().any(|e| {
            e.kind == "parakeet"
                && (models_dir.join(e.id).join("encoder.int8.onnx").is_file()
                    || models_dir.join(e.id).join("encoder.onnx").is_file())
        });

    // « auto » résout toujours vers quelque chose (factice en bout de
    // chaîne, jamais muet) : utilisable par construction.
    vec![
        EngineInfo {
            id: "auto".into(),
            compiled: true,
            usable: true,
        },
        EngineInfo {
            id: "whisper".into(),
            compiled: whisper_compiled,
            usable: whisper_usable,
        },
        EngineInfo {
            id: "nemo".into(),
            compiled: nemo_compiled,
            usable: nemo_usable,
        },
        EngineInfo {
            id: "parakeet".into(),
            compiled: parakeet_compiled,
            usable: parakeet_usable,
        },
    ]
}

#[tauri::command]
pub fn models_remove(id: String, shared: State<'_, UiShared>) -> Result<(), String> {
    let models_dir = Config::default_dir().join("models");
    if !MODEL_CATALOG.iter().any(|e| e.id == id) {
        return Err("Modèle inconnu.".to_string());
    }
    let path = models_dir.join(&id);
    let ok = if path.is_file() {
        std::fs::remove_file(&path)
    } else if path.is_dir() {
        std::fs::remove_dir_all(&path)
    } else {
        Ok(())
    };
    // Le retrait modifie le moteur effectif au prochain rebuild (RF-15) —
    // il doit se voir au journal, sinon la disparition d'un modèle reste
    // indémontrable (constat UX du 19/10 : modèle disparu sans trace).
    match &ok {
        Ok(()) => shared.logger.log(Event::DeviceEvent {
            kind: format!("modèle-retiré:{id}"),
        }),
        Err(_) => shared.logger.log(Event::DeviceEvent {
            kind: format!("modèle-retrait-échec:{id}"),
        }),
    }
    ok.map_err(|e| format!("{} : {e}", strings::ERR_WRITE))
}

#[tauri::command]
pub fn models_download_cancel(shared: State<'_, UiShared>) -> Result<(), String> {
    shared.download_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Téléchargement d'un modèle du catalogue : UNE action explicite de
/// l'utilisateur (RF-36), un seul à la fois, reprise `Range` supportée,
/// SHA-256 calculé et consigné dans `manifests.json` (RF-20). La connexion
/// est la SEULE voie réseau de Voxli — uniquement depuis cet écran, jamais
/// au démarrage ni en tâche de fond.
#[tauri::command]
pub fn models_download(
    app: AppHandle<Wry>,
    shared: State<'_, UiShared>,
    id: String,
) -> Result<(), String> {
    let Some(entry) = MODEL_CATALOG.iter().find(|e| e.id == id) else {
        return Err("Modèle inconnu.".to_string());
    };
    {
        let mut busy = lock_or_recover(&shared.download_busy);
        if *busy {
            return Err("Un téléchargement est déjà en cours.".to_string());
        }
        *busy = true;
    }
    shared.download_cancel.store(false, Ordering::SeqCst);

    let handle = app.clone();
    let cancel = Arc::clone(&shared.download_cancel);
    let id_owned = id.clone();
    let url = model_url(entry.id, entry.kind);
    let kind = entry.kind;
    let expected = entry.expected_sha256;
    let spawned = std::thread::Builder::new()
        .name("voxli-model-download".into())
        .spawn(move || {
            // P1-4 (audit) : même un panic du thread doit libérer le verrou
            // `download_busy` — sans remise, tout autre téléchargement
            // serait refusé « déjà en cours » jusqu'à redémarrage.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                download_model_thread(&handle, &id_owned, &url, kind, expected, &cancel)
            }));
            let reason = match result {
                Ok(Ok(())) => None,
                Ok(Err(reason)) => Some(reason),
                Err(_panic) => {
                    handle
                        .state::<UiShared>()
                        .logger
                        .log(Event::Error { module: "models".into(), code: 7 });
                    Some("erreur interne : téléchargement reconduit (l'application peut être relancée)".to_string())
                }
            };
            if let Some(reason) = reason {
                // Raison consignée au journal (RF-34 : métadonnées de
                // réseau, jamais de contenu dicté) — l'échec silencieux
                // du premier jet rendait le diagnostic impossible.
                handle.state::<UiShared>().logger.log(Event::DeviceEvent {
                    kind: format!(
                        "téléchargement-échec:{}",
                        reason.chars().take(96).collect::<String>()
                    ),
                });
                // Le succès est émis par le thread ; un échec (annulation,
                // réseau, extraction) doit lui aussi clore la progression
                // côté interface — jamais de barre bloquée.
                let _ = handle.emit(
                    "models://done",
                    serde_json::json!({ "id": id_owned, "ok": false, "reason": reason }),
                );
            }
            let shared = handle.state::<UiShared>();
            *lock_or_recover(&shared.download_busy) = false;
        });
    if let Err(e) = spawned {
        // P1-4 (audit) : le spawn a échoué AVANT d'exécuter le thread — le
        // verrou posé plus haut n'aurait jamais été libéré. On le remet et
        // on remonte l'erreur à l'interface.
        *lock_or_recover(&shared.download_busy) = false;
        return Err(format!("{} : {e}", strings::ERR_WORKER));
    }
    Ok(())
}

fn model_url(id: &str, kind: &str) -> String {
    match kind {
        "parakeet" => format!(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{id}.tar.bz2"
        ),
        "nemo" => format!(
            "https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/resolve/main/{id}"
        ),
        _ => format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{id}"),
    }
}

const DOWNLOAD_CHUNK: usize = 256 * 1024;

fn download_model_thread(
    app: &AppHandle<Wry>,
    id: &str,
    url: &str,
    kind: &str,
    expected: Option<&'static str>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let shared = app.state::<UiShared>();
    let models_dir = Config::default_dir().join("models");
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    // Partiel unique sous `{id}.part` pour tous les formats : un préfixe
    // contigu du flux serveur, il permet de reprendre sans tout re-télé-
    // charger. Le nom fantaisiste `.part.bz2` du premier jet est nettoyé
    // (fichier orphelin constaté en test réel, 192 Mo sans utilité).
    let tmp = models_dir.join(format!("{id}.part"));
    let legacy = models_dir.join(format!("{id}.part.bz2"));
    if legacy.is_file() {
        let _ = std::fs::remove_file(&legacy);
    }
    shared.logger.log(Event::DeviceEvent {
        kind: "téléchargement-démarré".into(),
    });

    // Échéance par INACTIVITÉ (30 s sans octet), pas de plafond global : un
    // téléchargement long sur ligne lente doit pouvoir finir (RF-20), une
    // connexion morte doit échouer tôt et reprendre ensuite (le délai
    // global de 600 s du premier jet coupait les téléchargements
    // volumineux).
    let agent: ureq::Agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();

    // Reprise AUTOMATIQUE (audit UX 19/10 : une connexion réinitialisée à
    // mi-transfert faisait échouer le téléchargement — l'utilisateur devait
    // re-cliquer, parfois en boucle, sans jamais finir). Chaque essai
    // reprend au `.part` déjà présent (plage `Range`) ; les erreurs réseau
    // réessayent avec un backoff croissant dans la limite de
    // MAX_TENTATIVES ; l'annulation reste immédiate à chaque instant.
    const MAX_TENTATIVES: u32 = 8;
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        if attempt > 1 {
            // Backoff annulable : 1, 2, 4, 8, 15, 30 puis 60 s.
            let wait = [1u64, 2, 4, 8, 15, 30, 60][((attempt - 2) as usize).min(6)];
            shared.logger.log(Event::DeviceEvent {
                kind: format!("téléchargement-reprise-auto:essai={attempt},attente={wait}s"),
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
            while std::time::Instant::now() < deadline {
                if cancel.load(Ordering::SeqCst) {
                    return Err("annulé".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        match transfer_attempt(app, id, url, cancel, &agent, &tmp) {
            Attempt::Complete => break,
            Attempt::AlreadyComplete => {
                return finalize_download(app, id, kind, expected, true);
            }
            Attempt::Fatal(e) => return Err(e),
            Attempt::Retryable(detail) => {
                shared.logger.log(Event::DeviceEvent {
                    kind: format!(
                        "téléchargement-réseau-interrompu:essai={attempt},{}",
                        detail.chars().take(96).collect::<String>()
                    ),
                });
                if attempt >= MAX_TENTATIVES {
                    let partio = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
                    return Err(format!(
                        "réseau : {detail} ({attempt} tentatives ; les {partio} o déjà reçus sont conservés, un nouvel essai reprendra)"
                    ));
                }
            }
        }
    }
    shared.logger.log(Event::DeviceEvent {
        kind: "téléchargement-fini".into(),
    });
    finalize_download(app, id, kind, expected, false)
}

/// Issue d'une tentative de transfert (voir `download_model_thread`).
enum Attempt {
    /// Flux serveur consommé intégralement : prêt pour la restitution.
    Complete,
    /// Le `.part` est déjà au complet : le serveur refuse la plage (416).
    AlreadyComplete,
    /// Erreur réseau : le `.part` est conservé, un nouvel essai reprendra.
    Retryable(String),
    /// Erreur fatale (annulation, disque) : abandon immédiat.
    Fatal(String),
}

/// UN essai de transfert : connexion avec plage `Range` posée à la taille
/// du `.part` courant, écriture par blocs, progression émise. Aucune
/// reprise ici — c'est le rôle de la boucle appelante.
fn transfer_attempt(
    app: &AppHandle<Wry>,
    id: &str,
    url: &str,
    cancel: &AtomicBool,
    agent: &ureq::Agent,
    tmp: &std::path::Path,
) -> Attempt {
    let shared = app.state::<UiShared>();
    let start_from = std::fs::metadata(tmp).map(|m| m.len()).unwrap_or(0);
    // P2 (audit) : un `.part` déjà complet répond 416 (plage hors contenu)
    // — ce n'est PAS un échec réseau : le fichier entier est déjà légitime-
    // ment là. Passer directement à la restitution ; sinon le fichier
    // partiel complet ne finissait jamais (« réseau : 416 » en boucle).
    let resp = match agent
        .get(url)
        .set("Range", &format!("bytes={start_from}-"))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::Status(416, _)) if start_from > 0 => {
            shared.logger.log(Event::DeviceEvent {
                kind: "téléchargement-reprise-déjà-complète".into(),
            });
            return Attempt::AlreadyComplete;
        }
        Err(e) => return Attempt::Retryable(format!("réseau : {e}")),
    };
    // Code du serveur : 206 = l'amorce de plage est accordée (reprise);
    // 200 = contenu ENTIER envoyé sans tenir compte de la plage — repartir
    // de zéro IMPÉRATIVEMENT, sinon le fichier serait corrompu (préfixe
    // conservé + flux complet). Sans ce test, aucune reprise n'était sûre.
    let content_len = resp
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let (from, expected_total) = if start_from > 0 && resp.status() == 206 {
        (start_from, content_len + start_from)
    } else if start_from > 0 {
        shared.logger.log(Event::DeviceEvent {
            kind: "téléchargement-reprise-refusée".into(),
        });
        (0u64, content_len)
    } else {
        (0u64, content_len)
    };

    let mut file = if from > 0 {
        match std::fs::OpenOptions::new().append(true).open(tmp) {
            Ok(f) => f,
            Err(e) => return Attempt::Fatal(e.to_string()),
        }
    } else {
        match std::fs::File::create(tmp) {
            Ok(f) => f,
            Err(e) => return Attempt::Fatal(e.to_string()),
        }
    };
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; DOWNLOAD_CHUNK];
    let mut downloaded = from;
    let mut next_emit = downloaded + 512 * 1024;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Attempt::Fatal("annulé".into());
        }
        use std::io::Read;
        let n = match reader.read(&mut buf) {
            Ok(0) => return Attempt::Complete,
            Ok(n) => n,
            Err(e) => return Attempt::Retryable(format!("réseau : {e}")),
        };
        use std::io::Write;
        if let Err(e) = file.write_all(&buf[..n]) {
            return Attempt::Fatal(e.to_string());
        }
        downloaded += n as u64;
        if expected_total > 0 && downloaded >= next_emit {
            next_emit = downloaded + 512 * 1024;
            let _ = app.emit(
                "models://progress",
                serde_json::json!({
                    "id": id,
                    "downloaded": downloaded,
                    "total": expected_total,
                }),
            );
        }
    }
}

/// Restitution finale commune (après un téléchargement OU après un 416 de
/// reprise complète) : extraction d'archive validée, renommage, empreinte
/// en flux, confrontation à la référence de confiance, manifeste, signal
/// au worker — le moteur se recharge sans redémarrage (RF-15).
fn finalize_download(
    app: &AppHandle<Wry>,
    id: &str,
    kind: &str,
    expected: Option<&'static str>,
    resumed_complete: bool,
) -> Result<(), String> {
    let shared = app.state::<UiShared>();
    let models_dir = Config::default_dir().join("models");
    let is_archive = kind == "parakeet";
    let dest = models_dir.join(id);
    let tmp = models_dir.join(format!("{id}.part"));

    if is_archive {
        // P1-5 (audit) : les ENTRÉES de l'archive sont validées AVANT
        // extraction — un membre absolu ou « ../ » ne doit jamais sortir du
        // dossier cible, quelle que soit la politique du tar système.
        validate_archive_members(&tmp)?;
        // Le répertoire cible doit exister AVANT `tar -C` : l'oubli du
        // premier jet faisait échouer toute extraction (défaut constaté
        // en test réel, l'archive complète restait inexploitable).
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        let out_str = dest.display().to_string();
        let input = tmp.display().to_string();
        let status = std::process::Command::new("tar")
            .args(["-xjf", &input, "-C", &out_str])
            .status()
            .map_err(|e| format!("extraction : {e}"))?;
        // Le partiel/archivé n'est supprimé que si l'extraction a réussi :
        // en cas d'échec, les octets déjà reçus restent réutilisables
        // (RF-20). EXCEPTION : si le fichier était déjà complet (reprise
        // 416), une archive cassée ré-échouerait à l'identique en boucle —
        // on supprime le fichier pour repartir d'un téléchargement sain.
        if !status.success() {
            if resumed_complete {
                let _ = std::fs::remove_file(&tmp);
            }
            return Err("extraction de l'archive impossible (tar requis)".to_string());
        }
        let _ = std::fs::remove_file(&tmp);
        // L'archive contient un dossier du même nom : ramener les fichiers
        // au niveau du dossier choisi (dest). Correction silencieuse si
        // déjà au bon format.
        if !dest.join("tokens.txt").is_file() {
            if let Ok(entries) = std::fs::read_dir(&dest) {
                for e in entries.flatten() {
                    let inner = e.path();
                    if inner.join("tokens.txt").is_file() {
                        for f in [
                            "encoder.int8.onnx",
                            "decoder.int8.onnx",
                            "joiner.int8.onnx",
                            "tokens.txt",
                            "encoder.onnx",
                            "decoder.onnx",
                            "joiner.onnx",
                        ] {
                            let src = inner.join(f);
                            if src.is_file() {
                                let _ = std::fs::rename(&src, dest.join(f));
                            }
                        }
                        break;
                    }
                }
            }
        }
        // P1-5 (audit, barrière post-extraction) : l'arbre extrait doit
        // rester canoniquement sous `dest` — attrape ce que les noms
        // seuls ne voient pas (liens symboliques, encodages exotiques).
        ensure_tree_within(&dest)?;
    } else {
        std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    }

    // Empreinte EN FLUX (P1-2 audit : jamais le modèle entier en RAM) —
    // digeste de dossier déterministe pour Parakeet (P1-1 audit : ne
    // JAMAIS lire un répertoire comme un fichier), fichier sinon.
    let sha = if is_archive {
        directory_digest(&dest)?
    } else {
        streamed_sha256(&dest)?
    };
    // P1-3 (audit) : confrontation à la référence de confiance quand elle
    // existe ; une incohérence est une panne bloquante (fichier effacé),
    // pas un avertissement noyé dans les logs.
    let verified = match expected {
        Some(expected) => {
            if !sha.eq_ignore_ascii_case(expected) {
                shared.logger.log(Event::Error {
                    module: "models".into(),
                    code: 8,
                });
                if dest.is_dir() {
                    let _ = std::fs::remove_dir_all(&dest);
                } else {
                    let _ = std::fs::remove_file(&dest);
                }
                return Err(format!(
                    "empreinte SHA-256 incohérente avec la référence de confiance ({sha} ≠ {expected})."
                ));
            }
            shared.logger.log(Event::DeviceEvent {
                kind: "empreinte-vérifiée".into(),
            });
            true
        }
        None => {
            // Aucun digest de confiance publié pour ce modèle ; l'empreinte
            // calculée reste CONSIGNÉE (vérifiable par l'utilisateur,
            // RF-20) — elle n'est pas présentée comme une garantie
            // d'authenticité (procédure docs/11 §3).
            shared.logger.log(Event::DeviceEvent {
                kind: "empreinte-consignée".into(),
            });
            false
        }
    };
    let manifests_path = models_dir.join("manifests.json");
    let mut root = serde_json::json!({
        "entries": serde_json::Value::Object(
            std::fs::read_to_string(&manifests_path)
                .ok()
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
                .map(|v| {
                    v.get("entries").cloned().unwrap_or(serde_json::json!({}))
                })
                .unwrap_or(serde_json::json!({}))
                .as_object()
                .cloned()
                .unwrap_or_default(),
        )
    });
    if let Some(obj) = root.get_mut("entries").and_then(|e| e.as_object_mut()) {
        let size = if is_archive {
            dir_size_bytes(&dest)
        } else {
            std::fs::metadata(&dest).ok().map(|m| m.len()).unwrap_or(0)
        };
        obj.insert(
            id.to_string(),
            serde_json::json!({
                "sha256": sha,
                "verified": verified,
                "sizeBytes": size,
                "downloadedAt": format!("epoch-{}", now_secs()),
            }),
        );
    }
    let _ = std::fs::write(
        &manifests_path,
        serde_json::to_string_pretty(&root).unwrap_or_default(),
    );
    let _ = app.emit("models://done", serde_json::json!({ "id": id, "ok": true }));
    // Le worker réévalue le moteur chargé : si le modèle fraîchement
    // installé le rend périmé (faux → réel), il recharge sans intervention
    // utilisateur (RF-15), et sans toucher à une dictée en cours.
    let _ = app.state::<UiShared>().tx.send(WorkerMsg::ModelInstalled);
    Ok(())
}

// ------------------------------------------------ empreintes en flux -----

/// Empreinte SHA-256 d'un fichier EN FLUX (P1-2 audit) : lecture par
/// morceaux de 1 Mio ; un GGUF ~708 Mo ne doit pas doubler la mémoire du
/// processus pour être digéré (l'ancien code lisait le fichier entier).
fn streamed_sha256(path: &std::path::Path) -> Result<String, String> {
    use sha2::Digest;
    use std::io::Read;
    const CHUNK: usize = 1024 * 1024;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_digest(&hasher.finalize()))
}

/// Préfixe du format d'empreinte de dossier (évolution future = nouveau
/// préfixe ; jamais le même nom pour une autre formule).
const DIR_DIGEST_V1: &str = "voxli-dir-digest-v1";

/// Digest déterministe d'un dossier modèle (Parakeet — P1-1 audit) :
/// fichiers triés par chemin relatif (séparateur `/`), chaque fichier
/// haché en flux, chemin et taille participant au digest — jamais un
/// `fs::read` d'un répertoire. La valeur n'est PAS une empreinte SHA-256
/// de fichier : c'est un format signé par son préfixe et consigné tel
/// quel au manifeste (`voxli-dir-digest-v1:…`).
fn directory_digest(dir: &std::path::Path) -> Result<String, String> {
    use sha2::Digest;
    use std::io::Read;

    fn collect(
        dir: &std::path::Path,
        rel: &str,
        out: &mut Vec<(String, std::path::PathBuf, u64)>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if p.is_dir() {
                collect(&p, &child, out)?;
            } else if p.is_file() {
                let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                out.push((child, p, len));
            }
        }
        Ok(())
    }

    let mut files: Vec<(String, std::path::PathBuf, u64)> = Vec::new();
    collect(dir, "", &mut files)?;
    if files.is_empty() {
        return Err("dossier de modèle vide : empreinte impossible".to_string());
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = sha2::Sha256::new();
    // En-tête : liste des chemins — le digest réagit à un ajout, un
    // retrait ou un renommage de fichier, pas seulement au contenu.
    hasher.update(DIR_DIGEST_V1.as_bytes());
    hasher.update(b"\n");
    let names: Vec<&str> = files.iter().map(|(r, _, _)| r.as_str()).collect();
    hasher.update(names.join("\n").as_bytes());
    hasher.update(b"\n");
    const CHUNK: usize = 1024 * 1024;
    let mut buf = vec![0u8; CHUNK];
    for (rel, path, len) in &files {
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        hasher.update(rel.as_bytes());
        hasher.update(b":");
        hasher.update(len.to_le_bytes());
        hasher.update(b"\n");
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    Ok(format!(
        "{DIR_DIGEST_V1}:{}",
        hex_digest(&hasher.finalize())
    ))
}

/// Digest SHA-256 brut → hexadécimal minuscule.
fn hex_digest(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// ----------------------------------------- validation de l'extraction ---

/// P1-5 (audit) : les noms d'ENTRÉES de l'archive sont listés (`tar -tjf`)
/// et validés AVANT l'extraction — jamais de dépendance aveugle à la
/// politique du `tar` système (bsdtar protège par défaut, GNU ancien non).
fn validate_archive_members(archive: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("tar")
        .args(["-tjf", &archive.display().to_string()])
        .output()
        .map_err(|e| format!("extraction : lecture de l'archive impossible ({e})"))?;
    if !out.status.success() {
        return Err("extraction : archive illisible (tar -t impossible)".to_string());
    }
    let listing = String::from_utf8_lossy(&out.stdout);
    for line in listing.lines() {
        if !archive_entry_name_safe(line) {
            return Err(format!(
                "extraction refusée : entrée d'archive suspecte « {} »",
                line.chars().take(72).collect::<String>()
            ));
        }
    }
    Ok(())
}

/// Nom d'entrée d'archive sain : relatif, sans « .. », sans séparateur
/// Windows ni « : » (préfixe de lecteur), sans NUL.
fn archive_entry_name_safe(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return false;
    }
    if name.contains('\\') || name.contains(':') || name.contains('\0') {
        return false;
    }
    name.split('/').all(|c| c != "..")
}

/// P1-5 (audit, barrière post-extraction) : tout fichier extrait doit
/// résoudre canoniquement DANS le dossier cible.
fn ensure_tree_within(root: &std::path::Path) -> Result<(), String> {
    let root_canon = root
        .canonicalize()
        .map_err(|e| format!("extraction : {e}"))?;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let canon = path
                .canonicalize()
                .map_err(|e| format!("extraction : {e}"))?;
            if !canon.starts_with(&root_canon) {
                return Err(format!(
                    "extraction refusée : sortie du dossier cible ({})",
                    canon.display()
                ));
            }
        }
    }
    Ok(())
}

/// P2 (audit) : le nom de modèle configuré est un NOM SIMPLE relatif au
/// dossier des modèles — jamais un chemin (pas de `/`, `\`, `..`, `:`,
/// pas de nom invisible commençant par un point). Empêche une config
/// modifiée de faire lire au moteur un fichier hors `%APPDATA%\Voxli\models`.
fn is_safe_model_name(name: &str) -> bool {
    // Les espaces de bord ne sont pas une variante légitime d'un nom :
    // «  .hidden.bin » (espace initiale) masque un fichier caché, et une
    // marge accidentelle pointerait ailleurs que le modèle voulu.
    if name != name.trim() {
        return false;
    }
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.starts_with('-')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && !name.split('/').any(|c| c == "..")
}

// --------------------------------------------------- historique RF-33 ---

#[derive(Serialize)]
pub struct HistoryItem {
    pub id: u64,
    pub text: String,
    pub engine: String,
    #[serde(rename = "createdMs")]
    pub created_ms: u64,
}

fn item_of(e: voxli_storage::Entry) -> HistoryItem {
    HistoryItem {
        id: e.id,
        text: e.text,
        engine: e.engine,
        created_ms: e.created_ms,
    }
}

/// Lecture bornée (RF-33) : les `n` entrées les plus récentes. Le store
/// chiffré décode côté utilisateur courant — l'IPC renvoie le texte de
/// l'historique : c'est l'exception **contenue** autorisée (contenu choisi
/// par l'utilisateur, supprimable à tout instant ; documentée docs/05 §3).
#[tauri::command]
pub fn history_query(shared: State<'_, UiShared>, n: Option<u32>) -> Vec<HistoryItem> {
    let guard = lock_or_recover(&shared.history);
    let Some(store) = guard.as_ref() else {
        return Vec::new();
    };
    let limit = n.unwrap_or(50).clamp(1, 200) as usize;
    store.recent(limit).into_iter().map(item_of).collect()
}

#[tauri::command]
pub fn history_delete(shared: State<'_, UiShared>, id: u64) -> Result<bool, String> {
    let mut guard = lock_or_recover(&shared.history);
    let Some(store) = guard.as_mut() else {
        return Ok(false);
    };
    Ok(store.delete(id))
}

#[tauri::command]
pub fn history_purge(shared: State<'_, UiShared>) -> Result<(), String> {
    let mut guard = lock_or_recover(&shared.history);
    if let Some(store) = guard.as_mut() {
        store.purge();
    }
    Ok(())
}

// ------------------------------------------------------ dictionnaire RF-23 ---

/// Exporte le dictionnaire d'un profil (CSV `cible|variante…`) au webview
/// (téléchargement via balise `<a download>`).
#[tauri::command]
pub fn dictionaries_export(shared: State<'_, UiShared>, profile: String) -> Result<String, String> {
    let cfg = lock_or_recover(&shared.config);
    let entries: Vec<String> = match profile.as_str() {
        "code" => cfg.profiles.code.dictionary.clone(),
        "email" => cfg.profiles.email.dictionary.clone(),
        "default" => cfg.profiles.default.dictionary.clone(),
        other => {
            return Err(format!("profile inconnu : {other}"));
        }
    };
    Ok(entries.join("\n"))
}

/// Importe un dictionnaire CSV (une entrée par ligne : `cible|v1|v2`).
#[tauri::command]
pub fn dictionaries_import(
    shared: State<'_, UiShared>,
    profile: String,
    csv: String,
) -> Result<usize, String> {
    let mut cfg = lock_or_recover(&shared.config).clone();
    let cleaned: Vec<String> = csv
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let count = cleaned.len();
    match profile.as_str() {
        "code" => cfg.profiles.code.dictionary = cleaned,
        "email" => cfg.profiles.email.dictionary = cleaned,
        "default" => cfg.profiles.default.dictionary = cleaned,
        other => return Err(format!("profile inconnu : {other}")),
    }
    cfg.save(&shared.config_path)
        .map_err(|e| format!("{} : {e}", strings::ERR_CONFIG_SAVE))?;
    *lock_or_recover(&shared.config) = cfg.clone();
    let _ = shared.tx.send(WorkerMsg::ConfigChanged(Box::new(cfg)));
    Ok(count)
}

// -------------------------------------------------------- RF-37 contexte ---

/// Aperçu de contexte : app + titre de la fenêtre au premier plan, JAMAIS
/// le contenu du champ (RF-37). Refus explicite hors mode autorisé.
#[tauri::command]
pub fn context_preview(shared: State<'_, UiShared>) -> Result<Option<(String, String)>, String> {
    let mode = lock_or_recover(&shared.config).privacy.context_read;
    if matches!(mode, voxli_config::ContextRead::None) {
        return Err("contexte désactivé (mode strict, RF-37)".into());
    }
    #[cfg(target_os = "windows")]
    {
        let excluded = lock_or_recover(&shared.config)
            .insertion
            .excluded_apps
            .clone();
        let reader = voxli_text::WindowsFocusReader::new(excluded);
        Ok(voxli_core::FocusReader::snapshot(&reader).map(|s| (s.app_id, s.title)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(None)
    }
}

// ------------------------------------------------------- export données ---

/// Export de données utilisateur (RF-33 : maîtrise) : copie la configuration,
/// les dictionnaires et le rapport de diagnostic dans un dossier fourni —
/// jamais de texte dicté ni de clé API dans l'export. Destination par
/// défaut : `Bureau\voxli-export-…`.
#[tauri::command]
pub fn data_export(
    shared: State<'_, UiShared>,
    destination: Option<String>,
) -> Result<String, String> {
    use std::fs;
    let dest = destination
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|u| PathBuf::from(u).join("Desktop"))
                .unwrap_or_else(std::env::temp_dir)
        })
        .join("Voxli");
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let dir = Config::default_dir();
    let stamp = format!("voxli-export-epoch-{}", now_secs());
    let out = dest.join(&stamp);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // Configuration (lisible, sans secrets).
    if let Ok(raw) = fs::read_to_string(dir.join("config.json")) {
        fs::write(out.join("config.json"), raw).map_err(|e| e.to_string())?;
    }
    // Empreintes des modèles installés (manifests).
    if let Ok(raw) = fs::read_to_string(dir.join("models").join("manifests.json")) {
        fs::create_dir_all(out.join("models")).ok();
        fs::write(out.join("models").join("manifests.json"), raw).map_err(|e| e.to_string())?;
    }
    // Rapport de support.
    let log_file = dir.join("logs").join("voxli.log");
    let raw = fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = voxli_diagnostics::export_report(&lines, shared.logger.uptime());
    fs::write(out.join("rapport-diagnostic.txt"), report).map_err(|e| e.to_string())?;
    Ok(out.display().to_string())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ------------------------------------------------------------ secours ---

/// Texte courant du tampon de secours ; `null` si aucun.
#[tauri::command]
pub fn rescue_last(shared: State<'_, UiShared>) -> Option<String> {
    lock_or_recover(&shared.last).clone()
}

/// « Coller à nouveau » (RF-31) — ré-insère le texte du tampon interne.
/// Exception documentée : le texte arbitraire ne peut pas entrer par IPC.
#[tauri::command]
pub fn rescue_paste_again(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(voxli_core::InputEvent::PasteAgain));
    Ok(())
}

/// « Réessayer » après panne moteur (RF-21) : l'orchestrateur relance le
/// pipeline avec l'audio qu'il a conservé.
#[tauri::command]
pub fn rescue_retry(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(voxli_core::InputEvent::Retry));
    Ok(())
}

#[tauri::command]
pub fn rescue_close(app: AppHandle<Wry>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("rescue") {
        let _ = w.close();
    }
    Ok(())
}

// -------------------------------------------------------- dictée ----

/// Annule une dictée en cours ou remet à zéro l'état de récupération
/// (l'audio retenu est libéré — invariant 3).
#[tauri::command]
pub fn cancel_dictation(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(voxli_core::InputEvent::Cancel));
    Ok(())
}

// ------------------------------------------------- diagnostic RF-34 ----

/// Exporte le rapport de support (rédigé par construction). Destination :
/// chemin fourni, sinon `Bureau\rapport-diagnostic-voxli.txt`.
#[tauri::command]
pub fn diagnostics_export(
    shared: State<'_, UiShared>,
    destination: Option<String>,
) -> Result<String, String> {
    let log_file = voxli_config::Config::default_dir()
        .join("logs")
        .join("voxli.log");
    let raw = std::fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = voxli_diagnostics::export_report(&lines, shared.logger.uptime());
    let path = destination.map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(|u| PathBuf::from(u).join("Desktop"))
            .unwrap_or_else(std::env::temp_dir)
            .join("rapport-diagnostic-voxli.txt")
    });
    std::fs::write(&path, report).map_err(|e| format!("{} : {e}", strings::ERR_WRITE))?;
    Ok(path.display().to_string())
}

// ------------------------------------------------------------ unitaires ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_safe_model_names() {
        assert!(is_safe_model_name("ggml-base-q5_1.bin"));
        assert!(is_safe_model_name("model-1.gguf"));
        assert!(!is_safe_model_name(""));
        assert!(!is_safe_model_name("."));
        assert!(!is_safe_model_name(" .hidden.bin"));
        assert!(!is_safe_model_name("../escape.bin"));
        assert!(!is_safe_model_name("sub/dir/model.bin"));
        assert!(!is_safe_model_name("sub\\dir\\model.bin"));
        assert!(!is_safe_model_name("C:evil.bin"));
        let nul = "\u{0}";
        assert!(!is_safe_model_name(nul));
    }

    #[test]
    fn t_archive_entry_names() {
        assert!(archive_entry_name_safe("tokens.txt"));
        assert!(archive_entry_name_safe("model-1.0/encoder.int8.onnx"));
        assert!(archive_entry_name_safe("a/b/c/d.txt"));
        assert!(!archive_entry_name_safe(""));
        assert!(!archive_entry_name_safe("/etc/passwd"));
        assert!(!archive_entry_name_safe("../evil.txt"));
        assert!(!archive_entry_name_safe("model/../../evil.txt"));
        assert!(!archive_entry_name_safe("C:/evil.txt"));
        assert!(!archive_entry_name_safe("a\\b.txt"));
    }

    #[test]
    fn t_streamed_hash_matches_reference() {
        // Parité : le hachage en flux doit valoir exactement le SHA-256
        // d'une lecture globale (sha2 en une passe, à titre de référence).
        let dir = std::env::temp_dir().join("voxli-commands-test-hash");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("sample.bin");
        let data: Vec<u8> = (0..250_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file, &data).unwrap();
        let streamed = streamed_sha256(&file).unwrap();
        let reference = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(&data);
            h.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(streamed, reference);
        // Déterminisme.
        assert_eq!(streamed_sha256(&file).unwrap(), streamed);
    }

    #[test]
    fn t_directory_digest_is_stable_and_responsive() {
        let dir = std::env::temp_dir().join("voxli-commands-test-dir");
        let model = dir.join("sherpa-faux");
        let _ = std::fs::remove_dir_all(&model);
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("tokens.txt"), b"ab cd").unwrap();
        std::fs::write(model.join("encoder.int8.onnx"), b"esch").unwrap();
        let d1 = directory_digest(&model).unwrap();
        assert!(d1.starts_with("voxli-dir-digest-v1:"));
        // Stabilité (même contenu → même digeste, ordre indépendant de l'OS).
        assert_eq!(directory_digest(&model).unwrap(), d1);
        // Sensibilité : changer un octet d'un fichier change le digest.
        std::fs::write(model.join("tokens.txt"), b"ab ce").unwrap();
        assert_ne!(directory_digest(&model).unwrap(), d1);
        // Sensibilité : DÉPLACER un fichier change les chemins relatifs,
        // donc le digest (couvre le renommage P1-1).
        std::fs::create_dir_all(model.join("sous")).unwrap();
        std::fs::rename(
            model.join("encoder.int8.onnx"),
            model.join("sous/encoder.int8.onnx"),
        )
        .unwrap();
        assert_ne!(directory_digest(&model).unwrap(), d1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t_dir_size_sums_files() {
        let dir = std::env::temp_dir().join("voxli-commands-test-size");
        let model = dir.join("parakeet-faux");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("a"), vec![0u8; 1000]).unwrap();
        std::fs::write(model.join("b"), vec![0u8; 500]).unwrap();
        std::fs::create_dir_all(model.join("sub")).unwrap();
        std::fs::write(model.join("sub/c"), vec![0u8; 100]).unwrap();
        assert_eq!(dir_size_bytes(&model), 1600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
