//! Remote **BYOK** AI engine (Bring Your Own Key) — ADR-03 point 3,
//! feature `remote`, P4 (RF-36).
//!
//! Guards (in order of defense):
//! 1. `consented`: the assembly refuses to build this engine without the
//!    double consent (`privacy.consent.remoteAsr` **and** strict mode
//!    disabled). On the engine side, a redundant refusal
//!    (`ConsentMissing`);
//! 2. the API key never lives in `config.json`: injected at mount time
//!    from `voxli-secure` (DPAPI) — never logged, never present in the
//!    returned error;
//! 3. the caller invokes this engine only on an explicit action
//!    ("use the cloud for THIS transcription").
//!
//! Transport: OpenAI-compatible multipart (`/v1/audio/transcriptions`).
//! The encoded WAV stays in memory; it never bypasses the strict local
//! mode (`strictLocalMode=true` gate in the assembly, doc RF-14).

use std::time::Duration;

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};

pub struct RemoteEngine {
    /// Chosen provider (logged, never the content).
    pub provider: String,
    /// OpenAI-compatible API base (e.g. `https://api.example.com/v1`).
    pub base_url: String,
    /// Requested model (e.g. `gpt-4o-transcribe`).
    pub model: String,
    /// User API key — injected from voxli-secure AT MOUNT TIME,
    /// never persisted nor logged (ADR-07).
    pub api_key: String,
    /// Redundant guard: `false` ⇒ immediate error, zero bytes sent.
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
        // No network warm-up behind the user's back.
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
        // The WAV is labeled 16 kHz with no resampling (no local
        // resampler): any other rate would yield a time-shifted
        // transcription (time stretched/compressed) sent to a
        // third-party server. The capture chain guarantees 16 kHz
        // (RF-19); out-of-contract usage is refused BEFORE any byte
        // is sent.
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
                    // No body returned to the app: the server may log.
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

/// Encodes mono 16 kHz f32 → in-memory WAV PCM16.
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
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // bytes/s
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
    for s in &samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Minimal multipart body: WAV file + optional language.
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
        // Expected size: 44 header bytes + 200 for samples.
        assert_eq!(wav.len(), 44 + 200);
        // The "data" byte count is correct.
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
        // Without consent: immediate error, ZERO network access (the test
        // need not stand up a server: the error is raised before).
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
        // Rate ≠ 16 kHz: refusal BEFORE any request (zero bytes leave
        // the machine) — `Runtime` with detail, never `Inference` (an
        // `Inference` here would mean the network request was attempted).
        let engine = RemoteEngine::new("openai", "http://127.0.0.1:1/v1", "m", "k".into(), true);
        // 100 ms at 44,100 Hz: out of contract.
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
