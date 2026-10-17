//! Plan d'insertion — **fonction pure** (docs/05 §2.4), traduction directe du
//! diagramme de décision de `docs/04` §4 :
//!
//! ```text
//! Cible valide ? ── non → fenêtre de secours
//! Champ sensible (IsPassword / app exclue / fenêtre élevée) ? ── oui → refus
//! ValuePattern disponible et éditable ? ── oui → niveau 1, puis repli
//! Profil presse-papiers → niveau 2 puis 3
//! Profil frappe directe → niveau 3 puis 2
//! Toujours : niveau 4 (secours) en dernier recours
//! ```

use plume_core::{FocusSnapshot, InsertLevel, InsertPlan, InsertionPolicy, Profile, RefuseReason};

pub fn plan_insertion(target: &FocusSnapshot, profile: &Profile) -> InsertPlan {
    // Fenêtre cible fermée ou perdue (RF-32) : secours immédiat.
    if !target.valid {
        return InsertPlan::refused(RefuseReason::TargetUnavailable);
    }
    // Fenêtre élevée (UIPI) ou champ de mot de passe (RF-29) : refus neutre.
    if target.elevated {
        return InsertPlan::refused(RefuseReason::SensitiveField);
    }
    if target.caps.is_password {
        return InsertPlan::refused(RefuseReason::SensitiveField);
    }

    let mut levels: Vec<InsertLevel> = Vec::with_capacity(4);
    // Niveau 1 : contrôle simple, éditable, non sensible (contrainte API
    // Windows : ValuePattern = mono-ligne uniquement).
    if target.caps.value_pattern_writable && target.caps.editable {
        levels.push(InsertLevel::ValuePattern);
    }
    match profile.insertion {
        InsertionPolicy::ClipboardFirst => {
            levels.push(InsertLevel::Clipboard);
            levels.push(InsertLevel::SendInput);
        }
        InsertionPolicy::TypingFirst => {
            levels.push(InsertLevel::SendInput);
            levels.push(InsertLevel::Clipboard);
        }
    }
    // Niveau 4 : aucun texte n'est jamais perdu (RF-31).
    levels.push(InsertLevel::Rescue);
    InsertPlan::ordered(levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plume_core::FieldCapabilities;

    fn caps(
        value_pattern: bool,
        editable: bool,
        password: bool,
        multiline: bool,
    ) -> FieldCapabilities {
        FieldCapabilities {
            value_pattern_writable: value_pattern,
            editable,
            is_password: password,
            multiline,
        }
    }

    fn snap(caps: FieldCapabilities) -> FocusSnapshot {
        FocusSnapshot {
            app_id: "notepad.exe".into(),
            title: "test".into(),
            caps,
            elevated: false,
            valid: true,
        }
    }

    #[test]
    fn t_simple_control_uses_value_pattern_first() {
        // Ligne 1 de la matrice : Notepad, contrôle simple → niveau 1.
        let plan = plan_insertion(&snap(caps(true, true, false, false)), &Profile::default());
        assert_eq!(plan.levels.first(), Some(&InsertLevel::ValuePattern));
        assert_eq!(plan.levels.last(), Some(&InsertLevel::Rescue));
        assert!(plan.refuse.is_none());
    }

    #[test]
    fn t_rich_editor_goes_to_clipboard() {
        // Lignes 2-3 : Word/Outlook, édition riche → presse-papiers.
        let plan = plan_insertion(&snap(caps(false, true, false, true)), &Profile::default());
        assert_eq!(plan.levels.first(), Some(&InsertLevel::Clipboard));
        assert_eq!(plan.levels.get(1), Some(&InsertLevel::SendInput));
    }

    #[test]
    fn t_code_profile_types_first() {
        // Ligne 9 : VS Code, profil verbatim → frappe directe d'abord.
        let profile = Profile {
            insertion: InsertionPolicy::TypingFirst,
            ..Default::default()
        };
        let plan = plan_insertion(&snap(caps(false, true, false, true)), &profile);
        assert_eq!(plan.levels.first(), Some(&InsertLevel::SendInput));
        assert_eq!(plan.levels.get(1), Some(&InsertLevel::Clipboard));
    }

    #[test]
    fn t_password_field_refused() {
        let plan = plan_insertion(&snap(caps(true, true, true, false)), &Profile::default());
        assert_eq!(plan.refuse, Some(RefuseReason::SensitiveField));
        assert!(
            plan.levels.is_empty(),
            "aucun niveau tenté dans un champ sensible"
        );
    }

    #[test]
    fn t_elevated_window_refused() {
        let mut s = snap(caps(false, true, false, false));
        s.elevated = true;
        let plan = plan_insertion(&s, &Profile::default());
        assert_eq!(plan.refuse, Some(RefuseReason::SensitiveField));
    }

    #[test]
    fn t_target_gone_refused() {
        let mut s = snap(caps(true, true, false, false));
        s.valid = false;
        let plan = plan_insertion(&s, &Profile::default());
        assert_eq!(plan.refuse, Some(RefuseReason::TargetUnavailable));
    }

    #[test]
    fn t_rescue_always_last() {
        // Quel que soit le chemin, le niveau 4 termine la liste.
        for vc in [true, false] {
            let plan = plan_insertion(&snap(caps(vc, true, false, false)), &Profile::default());
            assert_eq!(plan.levels.last(), Some(&InsertLevel::Rescue));
        }
    }

    #[test]
    fn t_full_cascade_order_documented() {
        // Ordre canonique docs/04 §4 pour un champ simple profil défaut.
        let plan = plan_insertion(&snap(caps(true, true, false, false)), &Profile::default());
        assert_eq!(
            plan.levels,
            vec![
                InsertLevel::ValuePattern,
                InsertLevel::Clipboard,
                InsertLevel::SendInput,
                InsertLevel::Rescue
            ]
        );
    }
}
