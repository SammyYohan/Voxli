//! Édition de liens vers le SDK NeMo-Speech.cpp quand la feature `nemo` est
//! active. Localisation : `$NEMO_SPEECH_PREFIX` ou l'installation par défaut
//! de l'installeur officiel (`%LOCALAPPDATA%\Programs\NeMoSpeech`).
//!
//! `build.rs` est exécuté par cargo (« pas de réseau, pas unsafe ») : ici la
//! seule action est de décrire au linker où trouver `nemo_speech_asr_c.lib`
//! et de copier les DLL du SDK à côté de l'exécutable final, pour que
//! `cargo run` / `cargo test` fonctionnent sans manipulation du PATH.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=NEMO_SPEECH_PREFIX");
    if env::var("CARGO_FEATURE_NEMO").is_err() {
        return;
    }

    let prefix = match env::var("NEMO_SPEECH_PREFIX") {
        Ok(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => {
            let local = env::var("LOCALAPPDATA").unwrap_or_default();
            if local.is_empty() {
                panic!(
                    "SDK NeMo-Speech.cpp introuvable : ni NEMO_SPEECH_PREFIX, \
                     ni LOCALAPPDATA. Installe le runtime (README §6)."
                );
            }
            PathBuf::from(local).join("Programs").join("NeMoSpeech")
        }
    };
    if !prefix.is_dir() {
        panic!(
            "NEMO_SPEECH_PREFIX n'est pas un répertoire : {} \
             (installe le SDK : scripts d'installation, README §6)",
            prefix.display()
        );
    }

    println!(
        "cargo:rustc-link-search=native={}",
        prefix.join("lib").display()
    );
    println!("cargo:rustc-link-lib=dylib=nemo_speech_asr_c");

    // DLL d'exécution à côté de la cible : ggml/llama + ABI C + runtime MSVC.
    let bin = prefix.join("bin");
    let target = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let manifest = env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
            PathBuf::from(manifest).join("../../target")
        });
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let dest = target.join(&profile);
    let mut copied = 0usize;
    if let Ok(entries) = std::fs::read_dir(&bin) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("dll"))
                != Some(true)
            {
                continue;
            }
            if let Some(name) = p.file_name() {
                if std::fs::copy(&p, dest.join(name)).is_ok() {
                    copied += 1;
                }
            }
        }
    }
    println!(
        "cargo:warning=NeMo-Speech.cpp : liaison depuis {} ({} DLL copiées)",
        prefix.display(),
        copied
    );
}
