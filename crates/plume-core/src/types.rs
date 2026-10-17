//! Types de données partagés. Ils vivent ici (docs/05 §6 : « types partagés
//! dans plume-core, jamais dans les implémentations »).

use std::time::Duration;

/// Fréquence cible canonique du pipeline : mono 16 kHz float32 (RF-12).
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// Segment audio mono, à la fréquence canonique, prêt pour le VAD et le moteur.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn duration(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.samples.len() as f64 / self.sample_rate as f64)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Niveau d'entrée normalisé (0.0 – 1.0) pour l'indicateur visuel (RF-09).
pub type Level = f32;

/// Identifiant de moteur, utilisé par les journaux et la licence (jamais de
/// contenu, uniquement des identifiants : RF-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineId {
    WhisperCpp,
    ParakeetOnnx,
    Remote,
    Fake,
}

impl EngineId {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineId::WhisperCpp => "whisper.cpp",
            EngineId::ParakeetOnnx => "parakeet-onnx",
            EngineId::Remote => "remote",
            EngineId::Fake => "fake",
        }
    }
}

/// Options de transcription (RF-16, RF-24). Le champ `initial_prompt` porte
/// l'amorce de vocabulaire métier ; il n'est jamais journalisé (RF-34).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TranscribeOptions {
    /// Code de langue (BCP-47 court, ex. `"fr"`) ou `None` pour la détection
    /// automatique — jamais utilisée sur un énoncé < 1,5 s (RF-16).
    pub language: Option<String>,
    /// Amorce de vocabulaire métier injectée au moteur (RF-24).
    pub initial_prompt: Option<String>,
    /// Mode verbatim : sortie moteur inchangée, aucune règle locale (RF-27).
    pub verbatim: bool,
    /// Nombre de threads d'inférence (RF-17). `None` = défaut du moteur.
    pub threads: Option<u8>,
}

/// Résultat de transcription. Le texte n'est jamais journalisé : seul cet
/// objet le porte, et il n'est sérialisé que vers l'historique local consenti
/// et la fenêtre de secours (RF-34).
#[derive(Clone, Debug, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub confidence: f32,
    pub duration: Duration,
    pub engine: EngineId,
    pub model: Option<String>,
}

/// Style de sortie demandé par le profil (RF-25, RF-27).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Texte brut du moteur, aucune réécriture.
    Verbatim,
    /// Règles locales de ponctuation et de casse françaises (niveau 1).
    #[default]
    Punctuated,
}

/// Politique d'insertion du profil (ADR-06) : ordre de préférence entre les
/// niveaux 2 et 3 de la cascade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InsertionPolicy {
    #[default]
    ClipboardFirst,
    TypingFirst,
}

/// Profil applicatif (RF-25). P0 : un profil unique par défaut.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub style: Style,
    pub insertion: InsertionPolicy,
}

/// Capacités du champ focalisé, lues via UIA (plume-text). Lecture seule :
/// l'orchestrateur ne connaît pas UIA.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldCapabilities {
    /// Un `ValuePattern` éditable est disponible (contrôle simple, non
    /// multiligne — contrainte API Windows, docs/03 §5).
    pub value_pattern_writable: bool,
    pub editable: bool,
    /// Champ de mot de passe (`IsPassword`) — insertion toujours refusée (RF-29).
    pub is_password: bool,
    pub multiline: bool,
}

/// Photographie de la cible de dictée, mémorisée **au démarrage** de la
/// dictée, pas au moment de l'insertion (RF-32).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusSnapshot {
    /// Identifiant d'application (nom d'exécutable, ex. `OUTLOOK.EXE`).
    pub app_id: String,
    pub title: String,
    pub caps: FieldCapabilities,
    /// Fenêtre élevée (admin) : injection impossible (UIPI) — refus explicite.
    pub elevated: bool,
    /// `false` si la fenêtre mémorisée n'existe plus au moment d'insérer.
    pub valid: bool,
}

impl FocusSnapshot {
    /// Décision de sensibilité (RF-29) : champ de mot de passe, application
    /// exclue, ou fenêtre élevée.
    pub fn is_sensitive(&self, excluded_apps: &[String]) -> bool {
        if self.caps.is_password || self.elevated {
            return true;
        }
        excluded_apps
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&self.app_id))
    }
}

/// Niveaux de la cascade d'insertion (ADR-06, RF-28).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertLevel {
    /// 1 — UI Automation `ValuePattern.SetValue` + vérification de relecture.
    ValuePattern,
    /// 2 — presse-papiers + `Ctrl+V`, avec sauvegarde/restauration (RF-30).
    Clipboard,
    /// 3 — frappe Unicode `SendInput`, caractère par caractère.
    SendInput,
    /// 4 — fenêtre de secours, texte pré-sélectionné.
    Rescue,
}

impl InsertLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            InsertLevel::ValuePattern => "value-pattern",
            InsertLevel::Clipboard => "clipboard",
            InsertLevel::SendInput => "sendinput",
            InsertLevel::Rescue => "rescue",
        }
    }
}

/// Motif de refus d'insertion (journalisé localement, sans contenu — RF-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefuseReason {
    /// Champ de mot de passe ou fenêtre élevée (RF-29).
    SensitiveField,
    /// Application dans la liste d'exclusion (RF-29).
    ExcludedApp,
    /// Fenêtre cible fermée ou introuvable (RF-32).
    TargetUnavailable,
}

impl RefuseReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefuseReason::SensitiveField => "sensitive-field",
            RefuseReason::ExcludedApp => "excluded-app",
            RefuseReason::TargetUnavailable => "target-unavailable",
        }
    }
}

/// Plan d'insertion produit par la fonction **pure** `plan_insertion`
/// (plume-text) : ordre des niveaux à tenter, ou motif de refus.
#[derive(Clone, Debug, PartialEq)]
pub struct InsertPlan {
    pub levels: Vec<InsertLevel>,
    pub refuse: Option<RefuseReason>,
}

impl InsertPlan {
    pub fn refused(reason: RefuseReason) -> Self {
        Self {
            levels: Vec::new(),
            refuse: Some(reason),
        }
    }

    pub fn ordered(levels: Vec<InsertLevel>) -> Self {
        Self {
            levels,
            refuse: None,
        }
    }
}

/// Compte rendu d'insertion : niveau retenu et, en cas de repli, le motif de
/// chaque tentative (jamais de contenu — RF-34).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsertReport {
    pub level_used: Option<InsertLevel>,
    /// `(niveau, code de motif)` pour chaque échec de niveau.
    pub attempts: Vec<(InsertLevel, &'static str)>,
}

/// Fonctionnalités soumises aux droits Free/Pro (RF-39). Jamais de dégradation
/// silencieuse : l'interface interrogeable est unique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    HistoryOnDisk,
    CloudPostProcessing,
    ExtraProfiles,
}

impl Feature {
    pub fn as_str(&self) -> &'static str {
        match self {
            Feature::HistoryOnDisk => "history-on-disk",
            Feature::CloudPostProcessing => "cloud-post-processing",
            Feature::ExtraProfiles => "extra-profiles",
        }
    }
}
