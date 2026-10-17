use plume_config::*;

#[test]
fn t_default_serializes_to_documented_schema() {
    let cfg = Config::default();
    let raw = serde_json::to_string_pretty(&cfg).unwrap();
    assert!(raw.contains("\"schema\": 4"));
    assert!(raw.contains("\"pushToTalk\": \"Ctrl+Alt+Space\""));
    assert!(raw.contains("\"preRollMs\": 400"));
    assert!(raw.contains("\"strictLocalMode\": true"));
    assert!(raw.contains("\"telemetry\": \"off\""));
    assert!(raw.contains("\"postProcessing\": \"rules\""));
    assert!(raw.contains("\"mode\": \"hold\""));
    assert!(raw.contains("\"consent\""));
    assert!(raw.contains("\"remoteAsr\": false"));
}

/// v3 → v4 : mode de déclenchement implicite (`hold`, compatibilité P0) et
/// consentements RF-36 absents → tout faux.
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

/// Le mode de déclenchement est mémorisé entre les sessions (RF-02c).
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
    // Fichier ancien : champ `cleanup`, pas de VAD explicite.
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
    let dir = std::env::temp_dir().join("plume-config-test");
    let path = dir.join("config.json");
    let cfg = Config::default();
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(cfg, loaded);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn t_default_excluded_apps_never_empty() {
    // RF-29 : la liste d'exclusion par défaut protège les gestionnaires de
    // mots de passe.
    assert!(default_excluded_apps().len() >= 3);
    assert!(default_excluded_apps()
        .iter()
        .any(|a| a.contains("keepass")));
}
