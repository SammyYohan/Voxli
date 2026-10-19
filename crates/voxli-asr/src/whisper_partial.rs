//! Partiels d'affichage pour les moteurs **batch offline** (whisper.cpp,
//! feature `whisper`).
//!
//! Principe : pendant la prise, un re-décodage **fenêtré** (dernières
//! secondes du tampon cumulé) est déclenché à intervalle borné et émis par
//! la voie streaming habituelle (`StreamUpdate::Partial`,
//! `dictation://partial`) — affichage seul, **jamais inséré ni journalisé**
//! (RF-34 + amendement ADR-02). Le texte définitif continue de venir de la
//! voie batch sur le tampon complet, qui reste l'autorité (docs/05 §3.1,
//! exception 3).
//!
//! Auto-désactivation : si un re-décodage dépasse [`MAX_DECODE_MS`], les
//! partiels s'arrêtent pour le reste de la session (machine lente) —
//! dégradation en douceur : l'utilisateur retrouve le comportement batch
//! sans coupure ni message d'erreur.

use std::sync::Arc;
use std::time::Instant;

use voxli_core::{AsrError, AudioBuffer, TranscribeOptions, Transcriptor, SAMPLE_RATE_HZ};

use crate::streaming::{StreamUpdate, StreamingEngine, StreamingSession};
use crate::whisper::WhisperCppEngine;

/// Fenêtre glissante décodée pour les partiels : le re-décodage ne porte que
/// sur les dernières secondes du tampon, pour borner le coût CPU par tick
/// (défaut et plafond : 8 s = [`PARTIAL_WINDOW_SAMPLES`] ; réglage avancé
/// possible par la variable d'environnement `VOXLI_PARTIAL_WINDOW_SEC`).
pub const PARTIAL_WINDOW_SAMPLES: usize = (SAMPLE_RATE_HZ as usize / 10) * 80; // 8 s
/// Audio neuf minimal (échantillons) entre deux re-décodages (1,2 s).
pub const MIN_NEW_SAMPLES: usize = (SAMPLE_RATE_HZ as usize / 10) * 12;
/// Intervalle minimal entre deux re-décodages (écrêtage du débit CPU).
pub const MIN_INTERVAL_MS: u128 = 1_500;
/// Un re-décodage plus long que ce plafond désactive les partiels de la
/// session (machine lente) : la finalisation batch prend le relais.
pub const MAX_DECODE_MS: u128 = 2_500;

/// Garde-fou mémoire du tampon partiel : 60 s d'audio canon 16 kHz.
const MAX_BUFFER_SAMPLES: usize = (SAMPLE_RATE_HZ as usize) * 60;

/// Durée (s) de la fenêtre de re-décodage partielle : 8 s par défaut,
/// réglable par la variable d'environnement `VOXLI_PARTIAL_WINDOW_SEC`
/// (réglage avancé, la configuration produit v5 n'y touche pas) —
/// bornée 2..8 pour que le coût CPU/par tick reste cadré et que les
/// tests restent hermétiques (ils passent par `with_thresholds`).
fn window_samples_from_env() -> usize {
    let sec: f64 = std::env::var("VOXLI_PARTIAL_WINDOW_SEC")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(8.0);
    let clamped = sec.clamp(2.0, 8.0);
    (SAMPLE_RATE_HZ as f64 * clamped) as usize
}

/// Fabrique de sessions partiels au-dessus de `WhisperCppEngine`.
///
/// Le tampon complet reste chez l'orchestrateur (RF-22) ; la session n'y
/// réplique que la fenêtre utile au re-décodage d'affichage.
pub struct WhisperPartialStreamer {
    engine: Arc<WhisperCppEngine>,
}

impl WhisperPartialStreamer {
    pub fn new(engine: Arc<WhisperCppEngine>) -> Self {
        Self { engine }
    }
}

impl StreamingEngine for WhisperPartialStreamer {
    fn open(
        &self,
        language: Option<&str>,
        verbatim: bool,
    ) -> Result<Box<dyn StreamingSession>, AsrError> {
        let engine = Arc::clone(&self.engine);
        let language = language.map(str::to_owned);
        let decode = move |audio: &AudioBuffer| -> Result<String, AsrError> {
            let opts = TranscribeOptions {
                language: language.clone(),
                initial_prompt: None,
                verbatim,
                threads: None, // défaut moteur (cf. RF-17 corrigé)
            };
            decode_window(&engine, audio, &opts)
        };
        Ok(Box::new(PartialSession::new(decode)))
    }
}

/// Décodage d'une fenêtre via le moteur partagé (chemin moteur réel ; les
/// tests injectent leur propre décodeur dans `PartialSession`).
fn decode_window(
    engine: &Arc<WhisperCppEngine>,
    audio: &AudioBuffer,
    opts: &TranscribeOptions,
) -> Result<String, AsrError> {
    engine.transcribe(audio, opts).map(|t| {
        // Hallucination de silence (RF-18) : pas de partiel du tout.
        WhisperCppEngine::filter_hallucination(&t.text).unwrap_or_default()
    })
}

/// État de session partiels, découplé du moteur : `decode` est injecté
/// (moteur réel ou double factice de test).
pub struct PartialSession<D>
where
    D: FnMut(&AudioBuffer) -> Result<String, AsrError>,
{
    decode: D,
    buffer: Vec<f32>,
    /// Longueur de tampon déjà couverte par un décodage (audio neuf = delta).
    decoded_until: usize,
    last_decode_at: Option<Instant>,
    /// Dernier texte partiel décodé (source d'affichage du snapshot) —
    /// voir `snapshot()` ci-dessous.
    last_text: String,
    /// Plafond de durée d'un décodage (production : [`MAX_DECODE_MS`]).
    max_decode_ms: u128,
    /// Intervalle minimal entre décodages (production : [`MIN_INTERVAL_MS`]).
    min_interval_ms: u128,
    /// Audio neuf minimal avant un re-décodage (production : 1,2 s).
    min_new_samples: usize,
    /// Longueur de la fenêtre re-décodée (production : réglable via
    /// `VOXLI_PARTIAL_WINDOW_SEC`, défaut 8 s ; tests : constante exacte
    /// pour rester hermétiques).
    window_samples: usize,
    disabled: bool,
}

impl<D> PartialSession<D>
where
    D: FnMut(&AudioBuffer) -> Result<String, AsrError>,
{
    pub fn new(decode: D) -> Self {
        Self {
            decode,
            buffer: Vec::new(),
            decoded_until: 0,
            last_decode_at: None,
            last_text: String::new(),
            max_decode_ms: MAX_DECODE_MS,
            min_interval_ms: MIN_INTERVAL_MS,
            min_new_samples: MIN_NEW_SAMPLES,
            window_samples: window_samples_from_env(),
            disabled: false,
        }
    }

    /// Constructeur complet pour les tests (seuils resserrés).
    #[cfg(test)]
    fn with_thresholds(
        decode: D,
        max_decode_ms: u128,
        min_interval_ms: u128,
        min_new_samples: usize,
    ) -> Self {
        Self {
            decode,
            buffer: Vec::new(),
            decoded_until: 0,
            last_decode_at: None,
            last_text: String::new(),
            max_decode_ms,
            min_interval_ms,
            min_new_samples,
            window_samples: PARTIAL_WINDOW_SAMPLES,
            disabled: false,
        }
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }
}

impl<D> StreamingSession for PartialSession<D>
where
    D: FnMut(&AudioBuffer) -> Result<String, AsrError> + Send,
{
    fn push(&mut self, samples: &[f32], rate: u32) -> Result<(), AsrError> {
        if rate == SAMPLE_RATE_HZ {
            self.buffer.extend_from_slice(samples);
        } else {
            // Resample linéaire vers le canon 16 kHz (le moteur batch n'y
            // accepte QUE 16 kHz — RF-12). Chemin secondaire : le robinet
            // de capture alimente déjà en 16 kHz.
            let ratio = SAMPLE_RATE_HZ as f64 / rate as f64;
            if ratio.is_finite() && ratio > 0.0 {
                let out_len = (samples.len() as f64 * ratio).ceil() as usize;
                let start = self.buffer.len();
                self.buffer.resize(start + out_len.max(1), 0.0);
                for (i, out) in self.buffer[start..].iter_mut().enumerate() {
                    let src = i as f64 / ratio;
                    let left = src.floor() as usize;
                    let right = (left + 1).min(samples.len().saturating_sub(1));
                    let frac = (src - left as f64) as f32;
                    *out = samples[left] * (1.0 - frac) + samples[right] * frac;
                }
            }
        }
        // Garde-fou mémoire : au-delà de 60 s cumulés, tronquer le début.
        if self.buffer.len() > MAX_BUFFER_SAMPLES {
            let drop = self.buffer.len() - MAX_BUFFER_SAMPLES;
            self.buffer.drain(..drop);
            self.decoded_until = self.decoded_until.saturating_sub(drop);
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<Vec<StreamUpdate>, AsrError> {
        if self.disabled {
            return Ok(Vec::new());
        }
        let new_audio = self.buffer.len().saturating_sub(self.decoded_until);
        if new_audio < self.min_new_samples {
            return Ok(Vec::new());
        }
        if let Some(last) = self.last_decode_at {
            if last.elapsed().as_millis() < self.min_interval_ms {
                return Ok(Vec::new());
            }
        }
        // Fenêtre : uniquement la fin du tampon (coût CPU borné par tick,
        // durée réglable — voir `window_samples_from_env`).
        let window_len = self.window_samples.min(self.buffer.len());
        let start = self.buffer.len() - window_len;
        let window = AudioBuffer::new(self.buffer[start..].to_vec(), SAMPLE_RATE_HZ);
        let t0 = Instant::now();
        let text = (self.decode)(&window)?;
        let took_ms = t0.elapsed().as_millis();
        self.decoded_until = self.buffer.len();
        self.last_decode_at = Some(Instant::now());
        if took_ms > self.max_decode_ms {
            // Machine trop lente : stop propre des partiels pour la session.
            self.disabled = true;
            return Ok(Vec::new());
        }
        if text.is_empty() {
            return Ok(Vec::new());
        }
        self.last_text = text.clone();
        Ok(vec![StreamUpdate::Partial { text }])
    }

    fn finish(&mut self) -> Result<String, AsrError> {
        // Le texte définitif vient exclusivement de la voie batch (tampon
        // complet) : `take_result` garde son défaut `None`.
        Ok(String::new())
    }

    fn snapshot(&self) -> String {
        // Le consommateur (`worker.rs`, emit `dictation://partial`) lit le
        // snapshot comme source d'affichage : renvoyer le dernier re-déco-
        // dage. Le texte définitif reste la voie batch, sans seconde vérité
        // (RF-34 : affichage seul, jamais inséré ni journalisé).
        self.last_text.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    const US: usize = SAMPLE_RATE_HZ as usize / 10; // 100 ms d'audio

    /// Décodeur lent : le premier décodage passe le plafond -> auto-off.
    #[test]
    fn t_partial_disable_on_slow_decode() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = std::sync::Arc::clone(&calls);
        let mut sess = PartialSession::with_thresholds(
            move |_: &AudioBuffer| {
                counter.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(5));
                Ok("texte".to_string())
            },
            1, // max_decode_ms très bas (test)
            0, // pas de throttle
            0, // pas de minimum d'audio neuf
        );
        sess.push(&[0.0; US], SAMPLE_RATE_HZ).unwrap();
        let out = sess.drain().unwrap();
        assert!(out.is_empty(), "décodage trop lent -> rien n'est émis");
        assert!(sess.disabled(), "session auto-désactivée");
        assert_eq!(calls.load(Ordering::Relaxed), 1, "un seul décodage tenté");
        sess.push(&[0.0; US], SAMPLE_RATE_HZ).unwrap();
        assert!(sess.drain().unwrap().is_empty(), "plus jamais après off");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    /// Décodeur rapide : audio neuf requis, émission bornée à la fenêtre.
    #[test]
    fn t_partial_requires_new_audio_window_bounded() {
        let seen: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        let lense = &seen;
        let mut sess = PartialSession::with_thresholds(
            move |audio: &AudioBuffer| {
                lense.lock().unwrap().push(audio.samples.len());
                Ok("partiel-eco".to_string())
            },
            u128::MAX, // jamais trop lent
            0,         // pas de throttle
            10 * US,   // 1 s d'audio neuf minimum
        );
        // 0,5 s d'audio neuf : sous le seuil -> rien.
        sess.push(&[0.0; 5 * US], SAMPLE_RATE_HZ).unwrap();
        assert!(sess.drain().unwrap().is_empty());
        // Total 1,1 s : un partiel est émis.
        sess.push(&[0.0; 6 * US], SAMPLE_RATE_HZ).unwrap();
        let out = sess.drain().unwrap();
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], StreamUpdate::Partial { ref text } if text.contains("partiel")));
        // Sans audio neuf : pas de re-décodage.
        assert!(sess.drain().unwrap().is_empty());
        // La fenêtre finale (tampon > 8 s) ne décode que les 8 dernières s.
        sess.push(&[0.0; 70 * US], SAMPLE_RATE_HZ).unwrap();
        assert_eq!(sess.drain().unwrap().len(), 1);
        for len in seen.lock().unwrap().iter() {
            assert!(len <= &PARTIAL_WINDOW_SAMPLES, "fenêtre > 8 s");
        }
        assert_eq!(
            *seen.lock().unwrap().last().unwrap(),
            PARTIAL_WINDOW_SAMPLES,
            "appel final = fenêtre exacte de 8 s"
        );
        // Snapshot : le dernier partiel décodé (le vrai texte d'affichage).
        assert_eq!(sess.snapshot(), "partiel-eco");
        assert_eq!(sess.finish().unwrap(), "");
    }
}
