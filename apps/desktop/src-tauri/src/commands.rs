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

use plume_config::Config;
use plume_diagnostics::Logger;
use plume_license::Entitlement as LicenseEntitlement;

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
    pub history: Mutex<Option<Box<dyn plume_storage::HistoryStore>>>,
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
    let Some(key) = plume_license::embedded_public_key() else {
        return LicenseInfo::free();
    };
    let Ok(blob) = std::fs::read_to_string(path) else {
        return LicenseInfo::free();
    };
    let grace = std::time::Duration::from_secs(30 * 86_400);
    match plume_license::verify(
        &blob,
        &key,
        plume_license::PRODUCT,
        std::time::SystemTime::now(),
        grace,
    ) {
        Ok(ent) => license_info_from(ent),
        Err(_) => LicenseInfo::free(),
    }
}

fn license_info_from(ent: LicenseEntitlement) -> LicenseInfo {
    use plume_core::{Entitlement as _, Feature};
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
    let Some(key) = plume_license::embedded_public_key() else {
        return Err(
            "Aucune clé publique embarquée dans ce binaire — import impossible.".to_string(),
        );
    };
    let grace = std::time::Duration::from_secs(30 * 86_400);
    let ent = plume_license::verify(
        &blob,
        &key,
        plume_license::PRODUCT,
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
    plume_audio::list_input_devices()
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
    /// "whisper" | "parakeet".
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
}

/// Catalogue : (id, libellé, type, taille Mo, licence) — la provenance et
/// la licence sont visibles dans l'interface avant tout téléchargement
/// (attribution CC-BY-4.0, THIRD-PARTY-LICENSES.md).
const MODEL_CATALOG: &[(&str, &str, &str, Option<u64>, &str)] = &[
    (
        "ggml-base-q5_0.bin",
        "Whisper base (FR, quantisé)",
        "whisper",
        Some(57),
        "MIT — ggerganov/whisper.cpp",
    ),
    (
        "ggml-small-q5_0.bin",
        "Whisper small (FR, quantisé)",
        "whisper",
        Some(180),
        "MIT — ggerganov/whisper.cpp",
    ),
    (
        "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
        "Parakeet TDT v3 0.6B (int8, 25 langues)",
        "parakeet",
        Some(230),
        "CC-BY-4.0 — NVIDIA NeMo (attribution requise)",
    ),
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
        .map(|(id, label, kind, size_mb, license)| {
            let path = match *kind {
                "parakeet" => models_dir.join(id).join("tokens.txt"),
                _ => models_dir.join(id),
            };
            let meta = path
                .parent()
                .filter(|_| *kind == "parakeet")
                .and_then(|p| Some(p.to_path_buf()))
                .unwrap_or_else(|| path.clone());
            let installed = match *kind {
                "parakeet" => {
                    let dir = models_dir.join(id);
                    dir.join("encoder.int8.onnx").is_file() || dir.join("encoder.onnx").is_file()
                }
                _ => path.is_file(),
            };
            let size_bytes = if installed {
                std::fs::metadata(&meta).ok().map(|m| m.len())
            } else {
                None
            };
            ModelInfo {
                id: (*id).into(),
                label: (*label).into(),
                kind: (*kind).into(),
                size_mb: *size_mb,
                license: (*license).into(),
                installed,
                size_bytes,
                sha256: hash_of_manifest(id),
            }
        })
        .collect()
}

#[tauri::command]
pub fn models_remove(id: String) -> Result<(), String> {
    let models_dir = Config::default_dir().join("models");
    if !MODEL_CATALOG.iter().any(|(i, ..)| *i == id) {
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
/// est la SEULE voie réseau de Plume — uniquement depuis cet écran, jamais
/// au démarrage ni en tâche de fond.
#[tauri::command]
pub fn models_download(
    app: AppHandle<Wry>,
    shared: State<'_, UiShared>,
    id: String,
) -> Result<(), String> {
    let Some((url, kind)) = MODEL_CATALOG
        .iter()
        .find(|(i, ..)| *i == id)
        .map(|(i, _, k, ..)| (*i, *k))
        .and_then(|(i, k)| Some((model_url(&i, k), k)))
    else {
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
    std::thread::Builder::new()
        .name("plume-model-download".into())
        .spawn(move || {
            let res = download_model_thread(&handle, &id_owned, &url, kind, &cancel);
            if res.is_err() {
                // Le succès est émis par le thread ; un échec (annulation,
                // réseau, extraction) doit eux aussi clore la progression
                // côté interface — jamais de barre bloquée.
                let _ = handle.emit(
                    "models://done",
                    serde_json::json!({ "id": id_owned, "ok": false, "reason": res.unwrap_err() }),
                );
            }
            let shared = handle.state::<UiShared>();
            *lock_or_recover(&shared.download_busy) = false;
        })
        .map_err(|e| format!("{} : {e}", strings::ERR_WORKER))?;
    Ok(())
}

fn model_url(id: &str, kind: &str) -> String {
    match kind {
        "parakeet" => format!(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{id}.tar.bz2"
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
    cancel: &AtomicBool,
) -> Result<(), String> {
    let models_dir = Config::default_dir().join("models");
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let is_zip = kind == "parakeet";
    let (dest, tmp) = if is_zip {
        (
            models_dir.join(id),
            models_dir.join(format!("{id}.part.bz2")),
        )
    } else {
        (models_dir.join(id), models_dir.join(format!("{id}.part")))
    };
    let start_from = if is_zip || !tmp.exists() {
        0u64
    } else {
        std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0)
    };

    let reader = ureq::get(url)
        .timeout(std::time::Duration::from_secs(600))
        .set("Range", &format!("bytes={start_from}-"))
        .call()
        .map_err(|e| e.to_string())?;
    let total = reader
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let mut file = if start_from > 0 && !is_zip {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?
    } else {
        std::fs::File::create(&tmp).map_err(|e| e.to_string())?
    };
    let mut reader = reader.into_reader();
    let mut buf = vec![0u8; DOWNLOAD_CHUNK];
    let mut downloaded = start_from;
    let mut next_emit = downloaded + 512 * 1024;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("annulé".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if total > 0 && downloaded >= next_emit {
            next_emit = downloaded + 512 * 1024;
            let _ = app.emit(
                "models://progress",
                serde_json::json!({
                    "id": id,
                    "downloaded": downloaded,
                    "total": if start_from > 0 { total + start_from } else { total },
                }),
            );
        }
    }
    // Extraction / renommage final.
    if is_zip {
        let out = models_dir.join(id);
        let out_str = out.display().to_string();
        let input = tmp.display().to_string();
        let status = std::process::Command::new("tar")
            .args(["-xjf", &input, "-C", &out_str])
            .status()
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        if !status.success() {
            return Err("extraction de l'archive impossible (tar requis)".to_string());
        }
        // L'archive contient un dossier du même nom : ramener les fichiers
        // au niveau du dossier choisi (id). Correction silencieuse si déjà
        // au bon format.
        if !out.join("tokens.txt").is_file() {
            if let Ok(entries) = std::fs::read_dir(&out) {
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
                                let _ = std::fs::rename(&src, out.join(f));
                            }
                        }
                        break;
                    }
                }
            }
        }
    } else {
        std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    }
    // Empreinte consignée (RF-20 : vérifiable par l'utilisateur).
    let bytes = std::fs::read(&dest).map_err(|e| e.to_string())?;
    let sha = sha256_of(&bytes);
    drop(bytes);
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
    if let (Some(size), Some(obj)) = (
        std::fs::metadata(&dest).ok().map(|m| m.len()),
        root.get_mut("entries").and_then(|e| e.as_object_mut()),
    ) {
        obj.insert(
            id.to_string(),
            serde_json::json!({
                "sha256": sha,
                "sizeBytes": size,
                "downloadedAt": format!("epoch-{}", now_secs()),
            }),
        );
    }
    let _ = std::fs::write(
        &manifests_path,
        serde_json::to_string_pretty(&root).unwrap_or_default(),
    );
    let _ = app.emit("models://done", serde_json::json!({ "id": id }));
    Ok(())
}

fn sha256_of(data: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

fn item_of(e: plume_storage::Entry) -> HistoryItem {
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
    if matches!(mode, plume_config::ContextRead::None) {
        return Err("contexte désactivé (mode strict, RF-37)".into());
    }
    #[cfg(target_os = "windows")]
    {
        let excluded = lock_or_recover(&shared.config)
            .insertion
            .excluded_apps
            .clone();
        let reader = plume_text::WindowsFocusReader::new(excluded);
        Ok(plume_core::FocusReader::snapshot(&reader).map(|s| (s.app_id, s.title)))
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
/// défaut : `Bureau\plume-export-…`.
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
        .join("Plume");
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let dir = Config::default_dir();
    let stamp = format!("plume-export-epoch-{}", now_secs());
    let out = dest.join(&stamp);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // Configuration (lisible, sans secrets).
    if let Ok(raw) = fs::read_to_string(&dir.join("config.json")) {
        fs::write(out.join("config.json"), raw).map_err(|e| e.to_string())?;
    }
    // Empreintes des modèles installés (manifests).
    if let Ok(raw) = fs::read_to_string(&dir.join("models").join("manifests.json")) {
        fs::create_dir_all(out.join("models")).ok();
        fs::write(out.join("models").join("manifests.json"), raw).map_err(|e| e.to_string())?;
    }
    // Rapport de support.
    let log_file = dir.join("logs").join("plume.log");
    let raw = fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = plume_diagnostics::export_report(&lines, shared.logger.uptime());
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
        .send(WorkerMsg::Input(plume_core::InputEvent::PasteAgain));
    Ok(())
}

/// « Réessayer » après panne moteur (RF-21) : l'orchestrateur relance le
/// pipeline avec l'audio qu'il a conservé.
#[tauri::command]
pub fn rescue_retry(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(plume_core::InputEvent::Retry));
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
        .send(WorkerMsg::Input(plume_core::InputEvent::Cancel));
    Ok(())
}

// ------------------------------------------------- diagnostic RF-34 ----

/// Exporte le rapport de support (rédigé par construction). Destination :
/// chemin fourni, sinon `Bureau\rapport-diagnostic-plume.txt`.
#[tauri::command]
pub fn diagnostics_export(
    shared: State<'_, UiShared>,
    destination: Option<String>,
) -> Result<String, String> {
    let log_file = plume_config::Config::default_dir()
        .join("logs")
        .join("plume.log");
    let raw = std::fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = plume_diagnostics::export_report(&lines, shared.logger.uptime());
    let path = destination.map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(|u| PathBuf::from(u).join("Desktop"))
            .unwrap_or_else(std::env::temp_dir)
            .join("rapport-diagnostic-plume.txt")
    });
    std::fs::write(&path, report).map_err(|e| format!("{} : {e}", strings::ERR_WRITE))?;
    Ok(path.display().to_string())
}
