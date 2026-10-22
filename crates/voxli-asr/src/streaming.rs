//! In-process streaming (P2): utterance session fed with audio slices.
//! Convention: the core (`voxli-core`) stays untouched — its batch seam
//! (`Transcriptor`) keeps responsibility for the final text; streaming
//! serves the live display (transient partials, never written,
//! RF-34 + ADR-02 amendment).
//!
//! Why a trait here and not in `voxli-core`: the core state machine only
//! knows "whole buffer → transcription". Partials are an interface
//! concern; injecting them into the core would break its purity
//! (docs/05 §2.1) for a purely visual benefit.

use voxli_core::AsrError;

/// Update of a streaming session.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamUpdate {
    /// Partial hypothesis: replaces the suffix currently displayed,
    /// never enters the final text.
    Partial { text: String },
    /// Segment finalized by the engine: aggregated into the reference text.
    Final { text: String },
}

/// Streaming recognition session: driven BY ONE thread (the capture faucet
/// pushes, the same thread polls — SDK contract).
pub trait StreamingSession: Send {
    /// Pushes a mono f32 slice at `rate` (8–96 kHz; internal resampling).
    /// Does not trigger inference: `drain` collects the results that
    /// arrived since the last call.
    fn push(&mut self, samples: &[f32], rate: u32) -> Result<(), AsrError>;

    /// Updates available at this instant (may be empty).
    fn drain(&mut self) -> Result<Vec<StreamUpdate>, AsrError>;

    /// End of capture: final flush, bounded drain of the last segments,
    /// returns the engine's reference text.
    fn finish(&mut self) -> Result<String, AsrError>;

    /// Reference text + average confidence produced by the session, if it
    /// has any (Nemo: the accumulated streaming finals). `None` = nothing
    /// to reuse, the caller falls back to its batch channel.
    fn take_result(&self) -> Option<(String, f32)> {
        None
    }

    /// "Display" text at this instant: finalized segments + current
    /// partial hypothesis. Never logged nor kept (RF-34).
    fn snapshot(&self) -> String;

    /// The session auto-disabled its partials (machine too slow):
    /// the driver logs the shutdown exactly once in the journal —
    /// otherwise the disappearance of the partials would be silent for
    /// the user (UX finding of 19/10).
    fn is_disabled(&self) -> bool {
        false
    }
}

/// Streaming session factory — offline recognition goes through the
/// usual batch trait, in parallel.
pub trait StreamingEngine: Send + Sync {
    /// Opens a session. NB: the SDK allows independent calls on the same
    /// recognizer; the session proper is single-threaded.
    fn open(
        &self,
        language: Option<&str>,
        verbatim: bool,
    ) -> Result<Box<dyn StreamingSession>, AsrError>;
}

/// Concatenates a segment into the reference text, handling spaces.
pub(crate) fn append_piece(base: &mut String, piece: &str) {
    let trimmed = piece.trim();
    if trimmed.is_empty() {
        return;
    }
    if base.is_empty() {
        base.push_str(trimmed);
        return;
    }
    if !base.ends_with(' ') {
        base.push(' ');
    }
    base.push_str(trimmed);
}

// --------------------------------------------------------- fake session ---

/// Deterministic session without SDK (tests, CI, wiring demo).
/// Every 4th `push` emits a partial; drain converts it into a final.
pub struct FakeSession {
    pushes: usize,
    committed: String,
    tail: Option<String>,
}

impl FakeSession {
    pub fn new() -> Self {
        Self {
            pushes: 0,
            committed: String::new(),
            tail: None,
        }
    }
}

impl Default for FakeSession {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingSession for FakeSession {
    fn push(&mut self, _samples: &[f32], _rate: u32) -> Result<(), AsrError> {
        self.pushes += 1;
        if self.pushes.is_multiple_of(4) {
            self.tail = Some(format!("mot {}", self.pushes / 4));
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<Vec<StreamUpdate>, AsrError> {
        if let Some(t) = self.tail.take() {
            // Small fake utterance: the partial becomes final 3 pushes
            // later (the test expects this deterministic behavior).
            self.pushes += 3;
            append_piece(&mut self.committed, &t);
            return Ok(vec![StreamUpdate::Final { text: t }]);
        }
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<String, AsrError> {
        Ok(self.committed.clone())
    }

    fn snapshot(&self) -> String {
        let mut s = self.committed.clone();
        if let Some(t) = &self.tail {
            append_piece(&mut s, t);
        }
        s
    }
}

/// Fake streaming base for the application-wiring tests.
pub struct FakeStreamer;

impl StreamingEngine for FakeStreamer {
    fn open(
        &self,
        _language: Option<&str>,
        _verbatim: bool,
    ) -> Result<Box<dyn StreamingSession>, AsrError> {
        Ok(Box::new(FakeSession::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_session_partiels_deterministes() {
        let mut s = FakeSession::new();
        // 3 pushes: no update as long as the step is not reached.
        for _ in 0..3 {
            s.push(&[0.0; 160], 16_000).unwrap();
            assert!(s.drain().unwrap().is_empty());
            assert!(s.snapshot().is_empty());
        }
        // on the 4th push, a partial is pending display…
        s.push(&[0.0; 160], 16_000).unwrap();
        assert!(s.snapshot().contains("mot"), "partiel affichable attendu");
        // …and the drain consumes one Update::Final (the partial is
        // finalized).
        let updates = s.drain().unwrap();
        assert!(
            updates.len() == 1
                && matches!(updates[0], StreamUpdate::Final { ref text } if text.contains("mot")),
            "un final factice est attendu : {updates:?}"
        );
        // the snapshot reflects the committed text; finish returns it.
        assert_eq!(s.snapshot(), "mot 1");
        assert_eq!(s.finish().unwrap(), "mot 1");
    }
}
