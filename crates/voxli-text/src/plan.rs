//! Insertion planner — **pure function** (docs/05 §2.4), direct translation
//! of the `docs/04` §4 decision diagram:
//!
//! ```text
//! Valid target? ── no → rescue window
//! Sensitive field (IsPassword / excluded app / elevated window)? ── yes → refuse
//! ValuePattern available and editable? ── yes → level 1, then fallback
//! Clipboard profile → level 2 then 3
//! Direct-typing profile → level 3 then 2
//! Always: level 4 (rescue) as last resort
//! ```

use voxli_core::{FocusSnapshot, InsertLevel, InsertPlan, InsertionPolicy, Profile, RefuseReason};

pub fn plan_insertion(target: &FocusSnapshot, profile: &Profile) -> InsertPlan {
    // Target window closed or gone (RF-32): immediate rescue.
    if !target.valid {
        return InsertPlan::refused(RefuseReason::TargetUnavailable);
    }
    // Elevated window (UIPI) or password field (RF-29): neutral refusal.
    if target.elevated {
        return InsertPlan::refused(RefuseReason::SensitiveField);
    }
    if target.caps.is_password {
        return InsertPlan::refused(RefuseReason::SensitiveField);
    }

    let mut levels: Vec<InsertLevel> = Vec::with_capacity(4);
    // Level 1: simple control, editable, non-sensitive (Windows API
    // constraint: ValuePattern = single-line only).
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
    // Level 4: no text is ever lost (RF-31).
    levels.push(InsertLevel::Rescue);
    InsertPlan::ordered(levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxli_core::FieldCapabilities;

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
        // Matrix line 1: Notepad, simple control → level 1.
        let plan = plan_insertion(&snap(caps(true, true, false, false)), &Profile::default());
        assert_eq!(plan.levels.first(), Some(&InsertLevel::ValuePattern));
        assert_eq!(plan.levels.last(), Some(&InsertLevel::Rescue));
        assert!(plan.refuse.is_none());
    }

    #[test]
    fn t_rich_editor_goes_to_clipboard() {
        // Lines 2-3: Word/Outlook, rich editing → clipboard.
        let plan = plan_insertion(&snap(caps(false, true, false, true)), &Profile::default());
        assert_eq!(plan.levels.first(), Some(&InsertLevel::Clipboard));
        assert_eq!(plan.levels.get(1), Some(&InsertLevel::SendInput));
    }

    #[test]
    fn t_code_profile_types_first() {
        // Line 9: VS Code, verbatim profile → direct typing first.
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
        // Whatever the path, level 4 always ends the list.
        for vc in [true, false] {
            let plan = plan_insertion(&snap(caps(vc, true, false, false)), &Profile::default());
            assert_eq!(plan.levels.last(), Some(&InsertLevel::Rescue));
        }
    }

    #[test]
    fn t_full_cascade_order_documented() {
        // Canonical docs/04 §4 order for a simple field with the default profile.
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
