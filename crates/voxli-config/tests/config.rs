use voxli_config::*;

#[test]
fn t_default_serializes_to_documented_schema() {
    let cfg = Config::default();
    let raw = serde_json::to_string_pretty(&cfg).unwrap();
    assert!(raw.contains("\"schema\": 5"));
    assert!(raw.contains("\"pushToTalk\": \"Ctrl+Alt+Space\""));
    assert!(raw.contains("\"preRollMs\": 400"));
    assert!(raw.contains("\"strictLocalMode\": true"));
    assert!(raw.contains("\"telemetry\": \"off\""));
    assert!(raw.contains("\"postProcessing\": \"rules\""));
    assert!(raw.contains("\"mode\": \"hold\""));
    assert!(raw.contains("\"consent\""));
    assert!(raw.contains("\"remoteAsr\": false"));
    assert!(raw.contains("\"uiLanguage\": \"fr\""));
    assert!(raw.contains("\"compute\": \"auto\""));
}

/// CPU/GPU/Duo compute choice (additive v5): values preserved, an unknown
/// one kept as-is (the core normalizes it to `auto` at use time, RF-29),
/// serialization round-trip intact.
#[test]
fn t_compute_round_trip_and_unknown() {
    for v in ["cpu", "gpu", "duo"] {
        let mut cfg = Config::default();
        cfg.general.compute = v.into();
        let raw = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.general.compute, v);
    }

    let weird = r#"{ "schema": 5, "general": { "compute": "tpu" } }"#;
    let cfg = Config::from_json(weird).unwrap();
    assert_eq!(cfg.general.compute, "tpu");
}

/// v3 → v4: implicit trigger mode (`hold`, P0 compatibility) and absent
/// RF-36 consents → all false.
#[test]
fn t_migration_from_schema_3() {
    let old = r#"{
        "schema": 3,
        "general": { "postProcessing": "rules" },
        "hotkeys": { "pushToTalk": "Ctrl+Alt+Space" },
        "privacy": { "strictLocalMode": true }
    }"#;
    let cfg = Config::from_json(old).unwrap();
    assert_eq!(cfg.schema, CURRENT_SCHEMA);
    assert_eq!(cfg.hotkeys.mode, TriggerMode::Hold);
    assert!(!cfg.privacy.consent.remote_asr);
    assert!(!cfg.privacy.consent.cloud_post);
}

/// The trigger mode is persisted across sessions (RF-02c).
#[test]
fn t_trigger_mode_round_trip() {
    let mut cfg = Config::default();
    cfg.hotkeys.mode = TriggerMode::DoublePushLock;
    let raw = serde_json::to_string(&cfg).unwrap();
    let back: Config = serde_json::from_str(&raw).unwrap();
    assert_eq!(back.hotkeys.mode, TriggerMode::DoublePushLock);
    assert!(raw.contains("\"double-push-lock\""));
}

#[test]
fn t_round_trip() {
    let cfg = Config::default();
    let raw = serde_json::to_string(&cfg).unwrap();
    let back: Config = serde_json::from_str(&raw).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn t_migration_from_schema_1() {
    // Old file: `cleanup` field, no explicit VAD.
    let old = r#"{
        "schema": 1,
        "general": { "language": "fr", "cleanup": "none" },
        "hotkeys": { "pushToTalk": "Ctrl+Alt+Space" }
    }"#;
    let cfg = Config::from_json(old).unwrap();
    assert_eq!(cfg.schema, CURRENT_SCHEMA);
    assert_eq!(cfg.general.post_processing, PostProcessing::None);
    assert!(cfg.audio.vad.enabled);
    assert_eq!(cfg.audio.vad.min_speech_ms, 250);
}

#[test]
fn t_migration_from_schema_2() {
    let old = r#"{ "schema": 2, "general": { "postProcessing": "rules" } }"#;
    let cfg = Config::from_json(old).unwrap();
    assert_eq!(cfg.schema, CURRENT_SCHEMA);
    assert!(cfg.audio.vad.min_speech_ms >= 100);
}

/// v4 → v5: the UI language appears (`uiLanguage`, default `fr`) while
/// `language` keeps its single role as the ASR listening language (RF-17)
/// — the two settings become independent (i18n, docs/11).
#[test]
fn t_migration_from_schema_4() {
    let old = r#"{
        "schema": 4,
        "general": { "language": "en", "postProcessing": "rules" }
    }"#;
    let cfg = Config::from_json(old).unwrap();
    assert_eq!(cfg.schema, CURRENT_SCHEMA);
    // The existing listening language is preserved; the UI falls back to
    // the safe value (fr) until the user chooses.
    assert_eq!(cfg.general.language, "en");
    assert_eq!(cfg.general.ui_language, "fr");
    // Additive v5 compute choice: safe value "auto" on an old file.
    assert_eq!(cfg.general.compute, "auto");

    // A `uiLanguage` already present is never overwritten by the migration.
    let already = r#"{
        "schema": 4,
        "general": { "language": "fr", "uiLanguage": "en" }
    }"#;
    let cfg = Config::from_json(already).unwrap();
    assert_eq!(cfg.general.ui_language, "en");

    // The pair survives an intact serialization round-trip.
    let raw = serde_json::to_string(&cfg).unwrap();
    let back: Config = serde_json::from_str(&raw).unwrap();
    assert_eq!(back.general.ui_language, "en");
    assert_eq!(back.general.language, "fr");
}

#[test]
fn t_future_schema_rejected() {
    let future = r#"{ "schema": 99 }"#;
    assert!(Config::from_json(future).is_err());
}

#[test]
fn t_unknown_fields_tolerated() {
    let raw = r#"{ "schema": 3, "futureField": { "x": 1 } }"#;
    let cfg = Config::from_json(raw).unwrap();
    assert_eq!(cfg, Config::default());
}

#[test]
fn t_numeric_clamps() {
    let raw = r#"{ "schema": 3, "audio": { "preRollMs": 99999, "maxRecordingSeconds": 1 } }"#;
    let cfg = Config::from_json(raw).unwrap();
    assert!(cfg.audio.pre_roll_ms <= 1000);
    assert!(cfg.audio.max_recording_seconds >= 30);
}

#[test]
fn t_save_and_load_file() {
    let dir = std::env::temp_dir().join("voxli-config-test");
    let path = dir.join("config.json");
    let cfg = Config::default();
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(cfg, loaded);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn t_default_excluded_apps_never_empty() {
    // RF-29: the default exclusion list protects password managers.
    assert!(default_excluded_apps().len() >= 3);
    assert!(default_excluded_apps()
        .iter()
        .any(|a| a.contains("keepass")));
}
