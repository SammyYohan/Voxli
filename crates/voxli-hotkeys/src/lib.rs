//! `voxli-hotkeys` — global hotkey policy (ADR-05).
//!
//! The **pure** part (normalization, validation, conflicts) lives here and
//! is tested outside Windows. The actual registration (`RegisterHotKey` via
//! the `tauri-plugin-global-shortcut` plugin) lives in
//! `apps/desktop/src-tauri`: a single registration mechanism, never two.
//!
//! Windows facts (docs/03 ADR-05):
//! - `RegisterHotKey` fails if the combination belongs to another process —
//!   the basis of RF-03;
//! - some system combinations (`Win+L`, `Ctrl+Alt+Suppr`…) cannot be
//!   registered — the UI must steer users toward viable choices;
//! - key release detection is polled every ~50 ms.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("combinaison illisible : {0}")]
    Unparsable(String),
    #[error("combinaison réservée par Windows : {0}")]
    Reserved(String),
    #[error("la combinaison est déjà enregistrée par une autre application")]
    Conflict,
    #[error("enregistrement impossible : {0}")]
    Register(String),
}

/// Recognized non-modifier key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Letter(char),
    Digit(u8),
    Function(u8),
    Space,
    Tab,
    Enter,
    Backquote,
}

impl Key {
    pub fn parse(s: &str) -> Option<Key> {
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "space" | "espace" => Some(Key::Space),
            "enter" | "return" | "entrée" => Some(Key::Enter),
            "tab" | "tabulation" => Some(Key::Tab),
            "`" | "backquote" | "quote" => Some(Key::Backquote),
            _ => {
                let mut chars = lower.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_ascii_alphabetic() => Some(Key::Letter(c)),
                    (Some(c), None) if c.is_ascii_digit() => Some(Key::Digit(c as u8 - b'0')),
                    _ => lower
                        .strip_prefix("f")
                        .and_then(|n| n.parse::<u8>().ok())
                        .filter(|n| (1..=24).contains(n))
                        .map(Key::Function),
                }
            }
        }
    }

    pub fn name(&self) -> String {
        match self {
            Key::Letter(c) => c.to_ascii_uppercase().to_string(),
            Key::Digit(d) => d.to_string(),
            Key::Function(n) => format!("F{n}"),
            Key::Space => "Space".into(),
            Key::Tab => "Tab".into(),
            Key::Enter => "Enter".into(),
            Key::Backquote => "`".into(),
        }
    }
}

/// Modifiers + key combination, normalized (canonical order
/// `Ctrl+Shift+Alt+Win+Key`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Combo {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub win: bool,
    pub key: Option<Key>,
}

impl Combo {
    /// Tolerant parsing: `Ctrl + Alt + Espace`, `ctrl+alt+space`… (RF-01:
    /// the user writes the combination however they like).
    pub fn parse(input: &str) -> Result<Combo, HotkeyError> {
        let mut combo = Combo::default();
        for part in input.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "ctrl+" | "contrôle" => combo.ctrl = true,
                "shift" | "maj" => combo.shift = true,
                "alt" | "option" => combo.alt = true,
                "win" | "super" | "meta" => combo.win = true,
                other => {
                    if combo.key.is_some() {
                        return Err(HotkeyError::Unparsable(input.into()));
                    }
                    combo.key = Some(
                        Key::parse(other).ok_or_else(|| HotkeyError::Unparsable(input.into()))?,
                    );
                }
            }
        }
        Ok(combo)
    }

    /// Canonical serialization shown to the user.
    pub fn canonical(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.win {
            parts.push("Win".into());
        }
        parts.push(self.key.map(|k| k.name()).unwrap_or_default());
        parts.join("+")
    }

    /// Combinations that cannot be registered (reserved by Windows).
    /// `Ctrl+Alt+Suppr` is never delivered to applications; `Win+L` locks
    /// the session and cannot be redefined without a system policy.
    pub fn is_reserved(&self) -> bool {
        if self.win {
            match self.key {
                Some(Key::Letter('l')) => return true,
                // Win+1..9: the system's app launcher (the taskbar grabs
                // these combinations, never us).
                Some(Key::Digit(d)) if (1..=9).contains(&d) => return true,
                Some(Key::Letter('g'))
                | Some(Key::Letter('h'))
                | Some(Key::Letter('s'))
                | Some(Key::Letter('r'))
                | Some(Key::Letter('e'))
                | Some(Key::Letter('d'))
                | Some(Key::Letter('i'))
                | Some(Key::Letter('a'))
                | Some(Key::Letter('c'))
                | Some(Key::Letter('v'))
                | Some(Key::Letter('k'))
                | Some(Key::Letter('n'))
                | Some(Key::Letter('w'))
                | Some(Key::Letter('p'))
                | Some(Key::Letter('x')) => return true,
                _ => {}
            }
        }
        if self.ctrl && self.alt {
            if let Some(Key::Letter('d' | 'e' | 'f')) = self.key {
                return true; // Ctrl+Alt+Del family / ergonomics
            }
        }
        false
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical())
    }
}

/// Conflict resolution at registration time (RF-03) — pure policy:
/// 1. reserved combination → immediate refusal with suggested alternatives;
/// 2. otherwise, the actual attempt is made by the wiring (the
///    `global-shortcut` plugin, the only allowed mechanism: `main.rs`) —
///    every failure is logged THEN reported to the UI, never a panic
///    (ADR-05). The `Conflict`/`Register` variants remain exposed to this
///    pure API's polymorphism but are not constructed to date (the wiring
///    handles failure differently).
pub fn validate_before_register(candidate: &Combo) -> Result<Vec<Combo>, HotkeyError> {
    if candidate.key.is_none() {
        return Err(HotkeyError::Unparsable("aucune touche".into()));
    }
    if candidate.is_reserved() {
        return Err(HotkeyError::Reserved(candidate.canonical()));
    }
    // Alternatives suggested to the user when the registration fails.
    let alternatives = vec![
        Combo {
            ctrl: true,
            alt: true,
            key: candidate.key,
            ..Default::default()
        },
        Combo {
            ctrl: true,
            shift: true,
            key: candidate.key,
            ..Default::default()
        },
        Combo {
            alt: true,
            shift: true,
            key: candidate.key,
            ..Default::default()
        },
        Combo {
            ctrl: true,
            shift: true,
            alt: true,
            key: candidate.key,
            ..Default::default()
        },
    ];
    Ok(alternatives
        .into_iter()
        .filter(|c| !c.is_reserved())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_parse_and_canonical_round_trip() {
        let c = Combo::parse("Ctrl + Alt + Espace").unwrap();
        assert_eq!(
            c,
            Combo {
                ctrl: true,
                alt: true,
                key: Some(Key::Space),
                ..Default::default()
            }
        );
        assert_eq!(c.canonical(), "Ctrl+Alt+Space");
        let c2 = Combo::parse("shift+alt+z").unwrap();
        assert_eq!(c2.canonical(), "Shift+Alt+Z");
        let c3 = Combo::parse("Control+Alt+entrée").unwrap();
        assert!(c3.ctrl && c3.alt);
    }

    #[test]
    fn t_reserved_combinations_rejected() {
        assert!(Combo::parse("Win+L").unwrap().is_reserved());
        assert!(Combo::parse("Win+V").unwrap().is_reserved());
        assert!(!Combo::parse("Ctrl+Alt+Space").unwrap().is_reserved());
        assert!(!Combo::parse("Shift+Alt+Z").unwrap().is_reserved());
    }

    #[test]
    fn t_validation_gives_alternatives() {
        let reserved = Combo::parse("Win+L").unwrap();
        assert!(matches!(
            validate_before_register(&reserved),
            Err(HotkeyError::Reserved(_))
        ));
        let ok = Combo::parse("Ctrl+Alt+Space").unwrap();
        let alts = validate_before_register(&ok).unwrap();
        assert!(!alts.is_empty());
        assert!(alts.iter().all(|c| !c.is_reserved()));
    }

    #[test]
    fn t_unparsable_inputs() {
        assert!(Combo::parse("").is_err() || Combo::parse("").unwrap().key.is_none());
        assert!(Combo::parse("Ctrl+Pouet").is_err());
    }
}
