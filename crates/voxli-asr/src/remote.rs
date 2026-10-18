//! Moteur IA distant **BYOK** (Bring Your Own Key) — ADR-03 point 3,
//! feature `remote`, P4 (RF-36).
//!
//! Garde-fous (par ordre de défense) :
//! 1. `consented` : l'assemblage refuse de construire ce moteur sans le
//!    double consentement (`privacy.consent.remoteAsr` **et** mode strict
//!    désactivé). Côté moteur, un refus redondant (`ConsentMissing`) ;
//! 2. la clé API ne vit jamais dans `config.json` : injectée au montage
//!    depuis `voxli-secure` (DPAPI) — jamais journalisée, jamais dans
//!    l'erreur renvoyée ;
//! 3. l'appelante n'appelle ce moteur que sur action explicite ("utiliser
//!    le cloud pour CETTE transcription").
//!
//! Transport : multipart OpenAI-compatible (`/v1/audio/transcriptions`).
//! Le WAV encodé est en mémoire ; il ne contourne jamais le mode strict
//! local (`strictLocalMode=true` gate dans l'assemblage, doc RF-14).

use std::time::Duration;

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};

pub struct RemoteEngine {
    /// Fournisseur choisi (journalisé, jamais le contenu).
    pub provider: String,
    /// Base API OpenAI-compatible (ex. `https://api.example.com/v1`).
    pub base_url: String,
    /// Modèle demandé (ex. `gpt-4o-transcribe`).
    pub model: String,
    /// Clé API utilisateur — injectée depuis voxli-secure AU MONTAGE,
    /// jamais persistée ni journalisée (ADR-07).
    pub api_key: String,
    /// Garde redondant : `false` ⇒ erreur immédiate, aucun octet envoyé.
    pub consented: bool,
}

impl RemoteEngine {
    pub fn new(
        provider: &str,
        base_url: &str,
        model: &str,
        api_key: String,
        consented: bool,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key,
            consented,
        }
    }
}

impl Transcriptor for RemoteEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        // Aucun préchauffage réseau à l'insu de l'utilisateur.
        Ok(())
    }

    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        if !self.consented || self.api_key.is_empty() {
            return Err(AsrError::ConsentMissing);
        }
        // Le WAV est étiqueté 16 kHz sans rééchantillonnage (aucun resampler
        // local) : tout autre taux donnerait une transcription déphasée (temps
        // étiré/comprimé) envoyée à un serveur tiers. La chaîne de capture
        // garantit 16 kHz (RF-19) ; un usage hors contrat est refusé AVANT
        // tout octet envoyé.
        if audio.sample_rate != SAMPLE_RATE_HZ {
            return Err(AsrError::Runtime(format!(
                "taux {} Hz refusé : le mode distant exige {SAMPLE_RATE_HZ} Hz",
                audio.sample_rate
            )));
        }
        let wav = encode_wav_16k_mono(audio);
        let body = multipart_body(&self.model, opts.language.as_deref(), &wav);
        let url = format!("{}/audio/transcriptions", self.base_url);

        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={MULTI_BOUNDARY}"),
            )
            .timeout(Duration::from_secs(120))
            .send_bytes(&body)
            .map_err(|e| match e {
                ureq::Error::Status(_code, resp) => {
                    // Aucun corps renvoyé à l'app : le serveur peut logguer.
                    let _ = resp.into_string();
                    AsrError::Inference
                }
                _ => AsrError::Inference,
            })?;
        if response.status() != 200 {
            return Err(AsrError::Inference);
        }
        let text = serde_json::from_slice::<serde_json::Value>(
            &response
                .into_string()
                .map_err(|_| AsrError::Inference)?
                .into_bytes(),
        )
        .ok()
        .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
        .ok_or(AsrError::Inference)?;

        Ok(Transcript {
            text,
            confidence: 0.9,
            duration: audio.duration(),
            engine: EngineId::Remote,
            model: Some(self.model.clone()),
        })
    }

    fn id(&self) -> EngineId {
        EngineId::Remote
    }
}

const MULTI_BOUNDARY: &str = "----voxli-boundary-7f3b914a";

/// Encode mono 16 kHz f32 → WAV PCM16 en mémoire.
pub fn encode_wav_16k_mono(audio: &AudioBuffer) -> Vec<u8> {
    let rate = if audio.sample_rate == 0 {
        SAMPLE_RATE_HZ
    } else {
        audio.sample_rate
    };
    let samples: Vec<i16> = audio
        .samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32_767.0) as i16)
        .collect();
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // taille fmt
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // octets/s
    out.extend_from_slice(&2u16.to_le_bytes()); // bloc aligne
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
    for s in &samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Corps multipart minimal : fichier WAV + langue optionnelle.
pub fn multipart_body(model: &str, language: Option<&str>, wav: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(wav.len() + 512);
    let push_str = |s: &str, out: &mut Vec<u8>| {
        out.extend_from_slice(s.as_bytes());
    };
    push_str(
        &format!("--{MULTI_BOUNDARY}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n"),
        &mut out,
    );
    if let Some(lang) = language {
        push_str(
            &format!("--{MULTI_BOUNDARY}\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\n{lang}\r\n"),
            &mut out,
        );
    }
    push_str(
        &format!("--{MULTI_BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"dictation.wav\"\r\nContent-Type: audio/wav\r\n\r\n"),
        &mut out,
    );
    out.extend_from_slice(wav);
    push_str(&format!("\r\n--{MULTI_BOUNDARY}--\r\n"), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxli_core::SAMPLE_RATE_HZ;

    #[test]
    fn t_wav_encoder_header_and_size() {
        let samples = vec![0.5_f32; 100];
        let audio = AudioBuffer::new(samples, SAMPLE_RATE_HZ);
        let wav = encode_wav_16k_mono(&audio);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        // Taille attendue : 44 octets d'en-tête + 200 d'échantillons.
        assert_eq!(wav.len(), 44 + 200);
        // Le nombre d'octets "data" est correct.
        let dlen = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(dlen, 200);
    }

    #[test]
    fn t_wav_sample_values_roundtrip() {
        let audio = AudioBuffer::new(vec![1.0, -1.0, 0.0], 16_000);
        let wav = encode_wav_16k_mono(&audio);
        // samples[0] = 32767 (clamp), s[1] = -32767.
        let v = |i: usize| i16::from_le_bytes([wav[44 + i * 2], wav[45 + i * 2]]);
        assert_eq!(v(0), 32_767);
        assert_eq!(v(1), -32_767);
        assert_eq!(v(2), 0);
    }

    #[test]
    fn t_multipart_contains_model_and_file() {
        let wav = vec![0xAA; 8];
        let body = multipart_body("gpt-4o-transcribe", Some("fr"), &wav);
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("name=\"model\""));
        assert!(s.contains("gpt-4o-transcribe"));
        assert!(s.contains("name=\"language\""));
        assert!(s.contains("fr"));
        assert!(s.contains("filename=\"dictation.wav\""));
        assert!(s.ends_with("--\r\n"));
    }

    #[test]
    fn t_no_consent_sends_nothing() {
        // Sans consentement : erreur immédiate, AUCUN accès réseau (le test
        // ne monterait pas un serveur : l'erreur est levée avant).
        let engine = RemoteEngine::new("openai", "http://127.0.0.1:1/v1", "m", "".into(), false);
        let audio = AudioBuffer::new(vec![0.0; 160], SAMPLE_RATE_HZ);
        let opts = TranscribeOptions {
            language: Some("fr".into()),
            initial_prompt: None,
            verbatim: false,
            threads: None,
        };
        let err = engine.transcribe(&audio, &opts).unwrap_err();
        assert!(matches!(err, AsrError::ConsentMissing));
    }

    #[test]
    fn t_non_16k_rejected_before_network() {
        // Taux ≠ 16 kHz : refus AVANT toute requête (aucun octet ne quitte la
        // machine) — `Runtime` avec détail, jamais `Inference` (un `Inference`
        // ici signifierait que la requête réseau a été tentée).
        let engine = RemoteEngine::new("openai", "http://127.0.0.1:1/v1", "m", "k".into(), true);
        // 100 ms à 44 100 Hz : hors contrat.
        let audio = AudioBuffer::new(vec![0.0; 4_410], 44_100);
        let opts = TranscribeOptions {
            language: Some("fr".into()),
            initial_prompt: None,
            verbatim: false,
            threads: None,
        };
        let err = engine.transcribe(&audio, &opts).unwrap_err();
        assert!(
            matches!(err, AsrError::Runtime(_)),
            "usage hors contrat (taux ≠ 16 kHz) ⇒ Runtime"
        );
    }
}
