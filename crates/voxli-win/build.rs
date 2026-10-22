//! Linking against the NeMo-Speech.cpp SDK when the `nemo` feature is
//! active. Location: `$NEMO_SPEECH_PREFIX` or the official installer's
//! default location (`%LOCALAPPDATA%\Programs\NeMoSpeech`).
//!
//! `build.rs` is executed by cargo ("no network, no unsafe"): its only
//! actions are to tell the linker where to find `nemo_speech_asr_c.lib`
//! and to copy the SDK DLLs next to the final executable, so that
//! `cargo run` / `cargo test` work without PATH manipulation.

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

    // Runtime DLLs next to the target: ggml/llama + C ABI + MSVC runtime.
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
