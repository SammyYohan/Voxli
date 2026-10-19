//! Streaming in-process (P2) : session d'énoncé alimentée par des tranches
//! d'audio. Convention : le noyau (`voxli-core`) reste inchangé — sa couture
//! batch (`Transcriptor`) garde la responsabilité du texte définitif ;
//! le streaming sert à l'affichage en direct (partiels transitoires,
//! jamais écrits, RF-34 + amendement ADR-02).
//!
//! Pourquoi un trait ici et pas dans `voxli-core` : la machine à états du
//! noyau ne connaît que « buffer entier → transcription ». Les partiels sont
//! une préoccupation d'interface ; les injecter au noyau briserait sa pureté
//! (docs/05 §2.1) pour un bénéfice purement visuel.

use voxli_core::AsrError;

/// Mise à jour d'une session streaming.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamUpdate {
    /// Hypothèse partielle : remplace le suffixe en cours d'affichage,
    /// n'entre jamais dans le texte définitif.
    Partial { text: String },
    /// Segment finalisé par le moteur : agrégé au texte de référence.
    Final { text: String },
}

/// Session de reconnaissance streaming : pilotée par UN thread (le robinet
/// de capture pousse, le même thread interroge — contrat SDK).
pub trait StreamingSession: Send {
    /// Pousse une tranche mono f32 au débit `rate` (8–96 kHz ; resample
    /// interne). Ne déclenche pas l'inférence : `drain` récupère les
    /// résultats arrivés depuis le dernier appel.
    fn push(&mut self, samples: &[f32], rate: u32) -> Result<(), AsrError>;

    /// Mises à jour disponibles à cet instant (peut être vide).
    fn drain(&mut self) -> Result<Vec<StreamUpdate>, AsrError>;

    /// Fin de prise : flush final, aspiration bornée des derniers
    /// segments, retour du texte de référence du moteur.
    fn finish(&mut self) -> Result<String, AsrError>;

    /// Texte de référence + confiance moyenne produits par la session, si
    /// elle en a (Nemo : le final accumulé du streaming). `None` = rien à
    /// reprendre, l'appelant retombe sur sa voie batch.
    fn take_result(&self) -> Option<(String, f32)> {
        None
    }

    /// Texte « affichage » à cet instant : segments finalisés + hypothèse
    /// partielle courante. Jamais journalisé ni conservé (RF-34).
    fn snapshot(&self) -> String;
}

/// Fabrique de session streaming — la reconnaissance offline passe par le
/// trait batch habituel, parallèlement.
pub trait StreamingEngine: Send + Sync {
    /// Ouvre une session. NB : le SDK autorise des appels indépendants sur
    /// le même recognizer ; la session proprement dite est mono-thread.
    fn open(
        &self,
        language: Option<&str>,
        verbatim: bool,
    ) -> Result<Box<dyn StreamingSession>, AsrError>;
}

/// Concatène un segment au texte de référence en gérant les espaces.
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

// ------------------------------------------------------- session factice ---

/// Session déterministe sans SDK (tests, CI, démonstration du câblage).
/// Chaque 4e `push` émet un partiel ; le drain le convertit en final.
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
            // Petit énoncé factice : le partiel devient final 3 pushes plus
            // tard (le test attend ce comportement déterministe).
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

/// Base de streaming factice pour les tests du câblage applicatif.
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
        // 3 pushes : aucun update tant que le pas n'est pas atteint.
        for _ in 0..3 {
            s.push(&[0.0; 160], 16_000).unwrap();
            assert!(s.drain().unwrap().is_empty());
            assert!(s.snapshot().is_empty());
        }
        // au 4e push, un partiel figure en attente d'affichage…
        s.push(&[0.0; 160], 16_000).unwrap();
        assert!(s.snapshot().contains("mot"), "partiel affichable attendu");
        // …et le drain consomme un Update::Final (le partiel est finalisé).
        let updates = s.drain().unwrap();
        assert!(
            updates.len() == 1
                && matches!(updates[0], StreamUpdate::Final { ref text } if text.contains("mot")),
            "un final factice est attendu : {updates:?}"
        );
        // le snapshot reflète le texte commité ; finish le renvoie.
        assert_eq!(s.snapshot(), "mot 1");
        assert_eq!(s.finish().unwrap(), "mot 1");
    }
}
