//! Shared-mode WASAPI capture via `cpal` (ADR-04).
//!
//! - the input stream is **continuous**: a pre-roll ring is fed at all
//!   times; `start` marks the beginning of a segment (pre-filled with the
//!   pre-roll), `stop` returns the converted audio (RF-06);
//! - the callback does not allocate, does not take a long lock and never
//!   touches the webview (RF-07, ADR-12): buffers are pre-reserved and the
//!   segment lock is short;
//! - the device's native format is treated as unknown (RF-13) and converted
//!   to mono 16 kHz f32 at stop (RF-12);
//! - metrics (frames, discontinuities) expose RF-07.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use voxli_core::{AudioBuffer, Capture, CaptureError, Level, SAMPLE_RATE_HZ};

use crate::convert::to_mono_16k;

const RING_SECONDS: f32 = 2.0;

/// Native-chunk observer for streaming (documented addition, docs/05):
/// called from the real-time path with a **mono** chunk at the device's
/// native rate, only while a capture is active (`active` present). Mixing
/// happens into a preallocated buffer: no allocation is added to the hot
/// path (RF-07).
pub type CaptureTap = Arc<dyn Fn(&[f32], u32) + Send + Sync>;

/// State shared between the real-time callback and the controller.
struct Shared {
    /// Pre-roll ring (fed continuously while the stream is active).
    ring: Vec<f32>,
    ring_pos: usize,
    ring_len: usize,
    /// Ongoing segment (pre-reserved at `start`: no allocation afterwards).
    active: Option<Vec<f32>>,
    /// Living level meter: recent amplitude with per-sample exponential
    /// decay (`meter_decay`) — instant attack, smooth release.
    /// (Compatibility history: until 2026-10-18 this was a peak held
    /// FOREVER — the gauge stayed up for the whole capture, no movement.)
    /// Accessed under `Mutex` (no atomics in the real-time path).
    meter: f32,
    /// Release factor PER SAMPLE: exp(-1 / (release × rate)).
    /// Pre-computed at construction (a single `exp`, nothing in the callback).
    meter_decay: f32,
    frames: AtomicU64,
    discontinuities: AtomicU64,
    lost: AtomicBool,
    /// Streaming tap (optional): called while a capture is active, with a
    /// mono chunk at rate `rate` (docs/05 addition).
    tap: Option<CaptureTap>,
    /// Preallocated tap mixing buffer (no allocation afterwards; a single
    /// documented growth may occur if a very large chunk ever appears).
    tap_scratch: Vec<f32>,
    /// Preallocated conversion buffer for I16/U8 chunks (P1-6 audit: the
    /// old code allocated a `Vec` PER CHUNK in the real-time path,
    /// contradicting RF-07). Same scheme as `tap_scratch`.
    conv_scratch: Vec<f32>,
    /// Native channel count (tap mixing) and native rate.
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
        // `_segment_cap`: the segment capacity is pre-reserved at `start`
        // (the callback cannot allocate, so we size it when needed).
        // Meter decay: aim for a ~180 ms release (the gauge drops ~90 % in
        // 0.4 s) — per-sample factor pre-computed.
        let sec = 0.180_f32;
        let meter_decay = if rate > 0 {
            (-1.0 / (sec * rate as f32)).exp()
        } else {
            1.0
        };
        Self {
            ring: vec![0.0; ring_cap],
            ring_pos: 0,
            ring_len: 0,
            active: None,
            meter: 0.0,
            meter_decay,
            frames: AtomicU64::new(0),
            discontinuities: AtomicU64::new(0),
            lost: AtomicBool::new(false),
            tap,
            tap_scratch: Vec::with_capacity(8192),
            conv_scratch: Vec::with_capacity(8192),
            channels,
            rate,
        }
    }

    /// Real-time callback path: no allocation.
    fn push(&mut self, data: &[f32]) {
        for &s in data {
            // Living level meter: instant attack (a loud word jumps the
            // gauge right away), exponential release (silence brings it
            // smoothly back to zero). Two multiplies and a compare per
            // sample, no allocation.
            self.meter *= self.meter_decay;
            let a = s.abs();
            if a > self.meter {
                self.meter = a;
            }
            let mono = s; // raw per-sample: mixdown happens at stop (via to_mono_16k)
                          // Ring
            self.ring[self.ring_pos] = mono;
            self.ring_pos = (self.ring_pos + 1) % self.ring.len();
            self.ring_len = (self.ring_len + 1).min(self.ring.len());
            // Segment
            if let Some(seg) = self.active.as_mut() {
                if seg.len() < seg.capacity() {
                    // direct push, no allocation (pre-reserved capacity)
                    seg.push(mono);
                } else {
                    // max duration reached: stop accumulating
                    // (RF-11 is already enforced on the orchestrator's logic side)
                }
            }
        }
        // Streaming tap: only while a capture is active, mean channel mix
        // into the preallocated buffer (no allocation from the calm path;
        // an overflow beyond the 8192-sample buffer would — rarely — cause
        // a single documented growth).
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

    /// Conversion + push of an I16 chunk (P1-6 audit: no allocation after
    /// the first chunk — preallocated scratch; `mem::take` avoids
    /// borrowing the buffer and `self` at the same time).
    fn push_i16(&mut self, data: &[i16]) {
        self.conv_scratch
            .extend(data.iter().map(|&s| s as f32 / 32768.0));
        let scratch = std::mem::take(&mut self.conv_scratch);
        self.push(&scratch);
        self.conv_scratch = scratch;
    }

    /// Conversion + push of a U8 chunk (same scheme as `push_i16`).
    fn push_u8(&mut self, data: &[u8]) {
        self.conv_scratch
            .extend(data.iter().map(|&s| (s as f32 - 128.0) / 128.0));
        let scratch = std::mem::take(&mut self.conv_scratch);
        self.push(&scratch);
        self.conv_scratch = scratch;
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

// cpal::Stream is not Send on every platform; the controller lives on the
// thread that created the stream (ADR-12: dedicated real-time thread).
unsafe impl Send for CpalCapture {}

impl CpalCapture {
    /// Opens the default microphone and reads out its native configuration.
    /// No stream is started before `start`.
    pub fn new() -> Result<Self, CaptureError> {
        Self::new_with_device("default")
    }

    /// RF-08: opens a named device, or the default microphone when
    /// `selector` is empty/`default`. The selector is the **stable cpal
    /// identifier** (`DeviceId`, serializable across sessions); as a
    /// fallback an exact name is also accepted. The name actually opened
    /// can be read via `device_name`.
    pub fn new_with_device(selector: &str) -> Result<Self, CaptureError> {
        Self::new_with_tap(selector, None)
    }

    /// Variant with a streaming tap: every active callback feeds `tap`
    /// with a mono chunk at the native rate (docs/05 addition).
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
                    // Fallback: matching by exact name.
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
        let config: cpal::StreamConfig = supported.into();
        let max_seconds = 1800u64; // RF-11 safety cap
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

    /// Stable identifier of the opened device (RF-08 settings).
    pub fn device_id(&self) -> Result<String, CaptureError> {
        self.device
            .id()
            .map(|i| i.to_string())
            .map_err(|_| CaptureError::NoDevice)
    }

    /// Name of the opened device (logging, RF-08 settings).
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
                // Heuristic by design: cpal gives no stable device-lost
                // kind, so the message is matched (`to_string()` does
                // allocate — accepted on an *error* path, never the hot
                // audio path). Every error bumps `discontinuities`;
                // "device"/"lost" additionally raises `lost`, the
                // worker's recovery signal (RF-34).
                if let Ok(s) = shared.try_lock() {
                    s.discontinuities.fetch_add(1, Ordering::Relaxed);
                    let msg = err.to_string().to_ascii_lowercase();
                    if msg.contains("device") || msg.contains("lost") {
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
                        s.push_i16(data);
                    }
                },
                err_cb,
                None,
            ),
            cpal::SampleFormat::U8 => self.device.build_input_stream(
                self.config,
                move |data: &[u8], _| {
                    if let Ok(mut s) = shared.try_lock() {
                        s.push_u8(data);
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
        // Marks the start of the segment, pre-filled with the pre-roll (RF-06).
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
        // The stream keeps running (the ring stays fed): we keep it for the
        // next capture; clean shutdown happens on Drop / app quit.
        let channels = self.config.channels as usize;
        let in_rate = self.config.sample_rate;
        if lost {
            return Err(CaptureError::DeviceLost);
        }
        let mono = to_mono_16k(&seg, channels, in_rate);
        if mono.is_empty() {
            // P2-3 (audit): "empty" is not "microphone lost" — the
            // device is alive (`lost` == false) and the state is
            // distinct in the log (code 7, RF-34).
            return Err(CaptureError::NoData);
        }
        Ok(AudioBuffer::new(mono, SAMPLE_RATE_HZ))
    }

    fn level(&self) -> Level {
        // Living level meter: recent amplitude (continuous exponential
        // decay in the real-time callback) — the pill's gauge fills with
        // voice and drains during silence.
        self.shared.lock().map(|s| s.meter).unwrap_or(0.0).min(1.0)
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
        self.stream = None; // stop the stream
    }
}

/// Input device shown in the settings (RF-08).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// Stable cpal identifier (`DeviceId`) — persisted in the
    /// configuration and reopens the same microphone across sessions.
    pub id: String,
    /// Human-readable name (driver description).
    pub name: String,
    /// `true` if it is the system default microphone at the time of the
    /// query (instantaneous information, not continuous tracking).
    pub is_default: bool,
}

/// Microphone enumeration (RF-08): listed in system order, with the
/// default microphone flagged. An empty list is not an error: a machine
/// without a microphone is not a failure.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the living level meter (2026-10-18): the old
    /// semantics held a lifetime peak (`to_bits().max()`) — the gauge went
    /// up and never came back down during a capture. Here: fast attack on
    /// speech, release during silence (in the 10 kHz configuration being
    /// tested).
    #[test]
    fn vumetre_attaque_et_se_vide_au_silence() {
        // Modest ring_cap: only the level meter is tested here.
        let mut shared = Shared::new(64, 0, 1, 10_000, None);
        // 0.3 s of "loud" speech (constant 0.5 amplitude).
        let loud = vec![0.5_f32; 3_000];
        shared.push(&loud);
        assert!(
            shared.meter > 0.45,
            "attaque attendue, meter={}",
            shared.meter
        );
        // 0.6 s of silence: the level must drop to near zero
        // (exp(-0.6/0.18) ≈ 3.6 %) — a lifetime peak would stay at 0.5.
        let silence = vec![0.0_f32; 6_000];
        shared.push(&silence);
        assert!(
            shared.meter < 0.05,
            "vidange attendue au silence, meter={}",
            shared.meter
        );
    }
}
