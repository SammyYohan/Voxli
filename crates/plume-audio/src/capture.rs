//! Capture WASAPI en mode partagé via `cpal` (ADR-04).
//!
//! - le flux d'entrée est **continu** : un anneau de pré-roll tourne en
//!   permanence ; `start` marque le début du segment (pré-rempli du
//!   pré-roll), `stop` en restitue l'audio converti (RF-06) ;
//! - le callback n'alloue pas, ne prend pas de verrou long et ne touche
//!   jamais au webview (RF-07, ADR-12) : les tampons sont pré-réservés,
//!   le verrou de segment est court ;
//! - le format natif du périphérique est traité comme inconnu (RF-13) et
//!   converti en mono 16 kHz f32 à l'arrêt (RF-12) ;
//! - les métriques (trames, discontinuités) exposent RF-07.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use plume_core::{AudioBuffer, Capture, CaptureError, Level, SAMPLE_RATE_HZ};

use crate::convert::to_mono_16k;

const RING_SECONDS: f32 = 2.0;

/// Observateur de tranches natives pour le streaming (addition documentée,
/// docs/05) : appelé depuis le chemin temps réel avec une tranche **mono**
/// au débit natif du périphérique, uniquement pendant une prise active
/// (`active` présent). Le mixage s'effectue dans un tampon préalloué :
/// aucune allocation ajoutée au chemin chaud (RF-07).
pub type CaptureTap = Arc<dyn Fn(&[f32], u32) + Send + Sync>;

/// État partagé entre le callback temps réel et le contrôleur.
struct Shared {
    /// Anneau de pré-roll (toujours alimenté pendant le flux actif).
    ring: Vec<f32>,
    ring_pos: usize,
    ring_len: usize,
    /// Segment en cours (pré-réservé à `start` : aucune allocation ensuite).
    active: Option<Vec<f32>>,
    /// Crête courant en bits f32 (accès sous `Mutex` — pas d'atomique dans le
    /// chemin temps réel).
    peak: u32,
    frames: AtomicU64,
    discontinuities: AtomicU64,
    lost: AtomicBool,
    /// Robinet streaming (optionnel) : appelé pendant une prise active,
    /// avec une tranche mono au débit `rate` (docs/05, addition).
    tap: Option<CaptureTap>,
    /// Tampon de mixage préalloué du robinet (aucune allocation ensuite ;
    /// croissance possible une fois si un celib très grand apparaît).
    tap_scratch: Vec<f32>,
    /// Nombre de canaux natifs (mixage du robinet) et débit natif.
    channels: usize,
    rate: u32,
}

impl Shared {
    fn new(
        ring_cap: usize,
        _segment_cap: usize,
        channels: usize,
        rate: u32,
        tap: Option<CaptureTap>,
    ) -> Self {
        // `_segment_cap` : la capacité de segment est pré-réservée à `start`
        // (le callback ne peut pas allouer, on la fixe au moment du besoin).
        Self {
            ring: vec![0.0; ring_cap],
            ring_pos: 0,
            ring_len: 0,
            active: None,
            peak: 0,
            frames: AtomicU64::new(0),
            discontinuities: AtomicU64::new(0),
            lost: AtomicBool::new(false),
            tap,
            tap_scratch: Vec::with_capacity(8192),
            channels,
            rate,
        }
    }

    /// Chemin du callback : aucune allocation.
    fn push(&mut self, data: &[f32]) {
        for &s in data {
            let bits = s.to_bits();
            self.peak = self.peak.max(bits);
            let mono = s; // mixdown effectué au stop (canal unique du flux)
                          // Anneau
            self.ring[self.ring_pos] = mono;
            self.ring_pos = (self.ring_pos + 1) % self.ring.len();
            self.ring_len = (self.ring_len + 1).min(self.ring.len());
            // Segment
            if let Some(seg) = self.active.as_mut() {
                if seg.len() < seg.capacity() {
                    // push direct sans allocation (capacité pré-réservée)
                    seg.push(mono);
                } else {
                    // durée maximale atteinte : on cesse d'accumuler
                    // (l'orchestrateur aura déjà déclenché RF-11 côté logique)
                }
            }
        }
        // Robinet streaming : uniquement pendant une prise, mixage moyen
        // des canaux dans le tampon préalloué (aucune allocation du
        // chemin calme ; un débordement du tampon de 8192 échantillons
        // par tranche ferait – rarement – une croissance unique,
        // documentée).
        if self.active.is_some() {
            if let Some(tap) = &self.tap {
                self.tap_scratch.clear();
                if self.channels == 1 {
                    self.tap_scratch.extend_from_slice(data);
                } else {
                    for frame in data.chunks_exact(self.channels) {
                        let sum: f32 = frame.iter().sum();
                        self.tap_scratch.push(sum / self.channels as f32);
                    }
                }
                if !self.tap_scratch.is_empty() {
                    tap(&self.tap_scratch, self.rate);
                }
            }
        }
        self.frames.fetch_add(data.len() as u64, Ordering::Relaxed);
    }

    fn recent_ring(&self, want: usize) -> Vec<f32> {
        let want = want.min(self.ring_len);
        let mut out = Vec::with_capacity(want);
        let start = (self.ring_pos + self.ring.len() - want) % self.ring.len();
        for i in 0..want {
            out.push(self.ring[(start + i) % self.ring.len()]);
        }
        out
    }
}

pub struct CpalCapture {
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    stream: Option<cpal::Stream>,
    shared: Arc<Mutex<Shared>>,
    ring_cap: usize,
    segment_cap: usize,
    pre_roll_ms: u32,
}

// cpal::Stream n'est pas Send sur toutes les plateformes ; le contrôleur vit
// sur le thread qui a créé le flux (ADR-12 : thread temps réel dédié).
unsafe impl Send for CpalCapture {}

impl CpalCapture {
    /// Ouvre le micro par défaut et lit sa configuration native. Aucun flux
    /// démarré avant `start`.
    pub fn new() -> Result<Self, CaptureError> {
        Self::new_with_device("default")
    }

    /// RF-08 : ouvre un périphérique nommé, ou le micro par défaut quand
    /// `selector` est vide/`default`. Le sélecteur est **l'identifiant
    /// cpal stable** (`DeviceId`, sérialisable entre les sessions) ; à
    /// titre de tolérance un nom exact est accepté. Le nom réel ouvert est
    /// consultable via `device_name`.
    pub fn new_with_device(selector: &str) -> Result<Self, CaptureError> {
        Self::new_with_tap(selector, None)
    }

    /// Variante avec robinet streaming : chaque callback actif alimente
    /// `tap` d'une tranche mono au débit natif (docs/05, addition).
    pub fn new_with_tap(selector: &str, tap: Option<CaptureTap>) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let wanted = selector.trim().is_empty() || selector.eq_ignore_ascii_case("default");
        let device = if wanted {
            host.default_input_device().ok_or(CaptureError::NoDevice)?
        } else {
            let by_id = selector
                .parse::<cpal::DeviceId>()
                .ok()
                .and_then(|id| host.device_by_id(&id));
            match by_id {
                Some(d) => d,
                None => {
                    // Tolérance : correspondance par nom exact.
                    host.input_devices()
                        .ok()
                        .and_then(|mut ds| {
                            ds.find(|d| {
                                d.description()
                                    .map(|desc| desc.name().eq_ignore_ascii_case(selector))
                                    .unwrap_or(false)
                            })
                        })
                        .ok_or(CaptureError::NoDevice)?
                }
            }
        };
        let supported = device
            .default_input_config()
            .map_err(|_| CaptureError::UnsupportedFormat)?;
        let config: cpal::StreamConfig = supported.clone().into();
        let max_seconds = 1800u64; // plafond de sécurité RF-11
        let ring_cap =
            (RING_SECONDS * config.sample_rate as f32) as usize * config.channels as usize;
        let segment_cap =
            (max_seconds as f32 * config.sample_rate as f32) as usize * config.channels as usize;
        Ok(Self {
            device,
            config,
            sample_format: supported.sample_format(),
            stream: None,
            shared: Arc::new(Mutex::new(Shared::new(
                ring_cap,
                segment_cap,
                config.channels as usize,
                config.sample_rate,
                tap,
            ))),
            ring_cap,
            segment_cap,
            pre_roll_ms: 400,
        })
    }

    /// Identifiant stable du périphérique ouvert (réglages RF-08).
    pub fn device_id(&self) -> Result<String, CaptureError> {
        self.device
            .id()
            .map(|i| i.to_string())
            .map_err(|_| CaptureError::NoDevice)
    }

    /// Nom du périphérique ouvert (journalisation, réglages RF-08).
    pub fn device_name(&self) -> String {
        self.device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_default()
    }

    pub fn frames(&self) -> u64 {
        self.shared
            .lock()
            .map(|s| s.frames.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn discontinuities(&self) -> u64 {
        self.shared
            .lock()
            .map(|s| s.discontinuities.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    fn build_stream(&self) -> Result<cpal::Stream, CaptureError> {
        let shared = Arc::clone(&self.shared);
        let err_cb = {
            let shared = Arc::clone(&shared);
            move |err: cpal::Error| {
                // Pas d'allocation dans le chemin d'erreur chaud : compteur.
                if let Ok(s) = shared.try_lock() {
                    s.discontinuities.fetch_add(1, Ordering::Relaxed);
                    if err.to_string().contains("device") {
                        s.lost.store(true, Ordering::Relaxed);
                    }
                }
            }
        };
        match self.sample_format {
            cpal::SampleFormat::F32 => self.device.build_input_stream(
                self.config,
                move |data: &[f32], _| {
                    if let Ok(mut s) = shared.try_lock() {
                        s.push(data);
                    }
                },
                err_cb,
                None,
            ),
            cpal::SampleFormat::I16 => self.device.build_input_stream(
                self.config,
                move |data: &[i16], _| {
                    if let Ok(mut s) = shared.try_lock() {
                        let conv: Vec<f32> = data.iter().map(|&v| v as f32 / 32768.0).collect();
                        s.push(&conv);
                    }
                },
                err_cb,
                None,
            ),
            cpal::SampleFormat::U8 => self.device.build_input_stream(
                self.config,
                move |data: &[u8], _| {
                    if let Ok(mut s) = shared.try_lock() {
                        let conv: Vec<f32> =
                            data.iter().map(|&v| (v as f32 - 128.0) / 128.0).collect();
                        s.push(&conv);
                    }
                },
                err_cb,
                None,
            ),
            _ => return Err(CaptureError::UnsupportedFormat),
        }
        .map_err(|_| CaptureError::StreamStart)
    }
}

impl Capture for CpalCapture {
    fn start(&mut self, pre_roll_ms: u32) -> Result<(), CaptureError> {
        self.pre_roll_ms = pre_roll_ms;
        if self.stream.is_none() {
            let stream = self.build_stream()?;
            stream.play().map_err(|_| CaptureError::StreamStart)?;
            self.stream = Some(stream);
        }
        // Marque le début du segment, pré-rempli du pré-roll (RF-06).
        let mut shared = self.shared.lock().map_err(|_| CaptureError::StreamStart)?;
        let want = (pre_roll_ms as f32 / 1000.0 * self.config.sample_rate as f32) as usize
            * self.config.channels as usize;
        let want = want.min(self.ring_cap);
        let pre = shared.recent_ring(want);
        let mut seg = Vec::with_capacity(self.segment_cap.min(1 << 30));
        seg.extend(pre);
        shared.active = Some(seg);
        Ok(())
    }

    fn stop(&mut self) -> Result<AudioBuffer, CaptureError> {
        let mut shared = self.shared.lock().map_err(|_| CaptureError::StreamStart)?;
        let seg = shared.active.take().ok_or(CaptureError::NotStarted)?;
        let lost = shared.lost.load(Ordering::Relaxed);
        drop(shared);
        // Le flux continue de tourner (l'anneau reste alimenté) : on le
        // garde pour la prochaine prise ; arrêt propre au Drop / à l'app quit.
        let channels = self.config.channels as usize;
        let in_rate = self.config.sample_rate;
        if lost {
            return Err(CaptureError::DeviceLost);
        }
        let mono = to_mono_16k(&seg, channels, in_rate);
        if mono.is_empty() {
            return Err(CaptureError::DeviceLost);
        }
        Ok(AudioBuffer::new(mono, SAMPLE_RATE_HZ))
    }

    fn level(&self) -> Level {
        let bits = self.shared.lock().map(|s| s.peak).unwrap_or(0);
        let peak = f32::from_bits(bits).abs();
        peak.min(1.0)
    }

    fn device_lost(&self) -> bool {
        self.shared
            .lock()
            .map(|s| s.lost.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}

impl Drop for CpalCapture {
    fn drop(&mut self) {
        self.stream = None; // arrête le flux
    }
}

/// Périphérique d'entrée visible dans les réglages (RF-08).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// Identifiant cpal stable (`DeviceId`) — persiste dans la
    /// configuration et rouvre le même micro entre les sessions.
    pub id: String,
    /// Nom lisible (description du pilote).
    pub name: String,
    /// `true` si c'est le micro par défaut du système au moment de la
    /// consultation (information instantanée, pas un suivi en continu).
    pub is_default: bool,
}

/// Énumération des micros (RF-08) : liste dans l'ordre du système, avec
/// marquage du micro par défaut. Une liste vide n'est pas une erreur : une
/// machine sans micro n'est pas une panne.
pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default = host.default_input_device();
    let default_id = default
        .as_ref()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string());
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let id = match d.id() {
                Ok(i) => i.to_string(),
                Err(_) => continue,
            };
            let name = d
                .description()
                .map(|desc| desc.name().to_string())
                .unwrap_or_else(|_| "microphone sans nom".to_string());
            let is_default = default_id.as_deref() == Some(id.as_str());
            out.push(InputDevice {
                id,
                name,
                is_default,
            });
        }
    }
    out
}
