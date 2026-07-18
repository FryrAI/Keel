use super::*;
use crate::commands::init::templates;

/// Every instruction template that makes an auto-compile claim.
const CLAIMING_TEMPLATES: &[(&str, &str)] = &[
    ("claude-code", templates::CLAUDE_CODE_INSTRUCTIONS),
    ("letta", templates::LETTA_INSTRUCTIONS),
    ("gemini", templates::GEMINI_INSTRUCTIONS),
    ("cursor", templates::CURSOR_RULES),
    ("windsurf", templates::WINDSURF_RULES),
];

#[test]
fn test_claims_survive_when_on_edit_true() {
    for (tool, template) in CLAIMING_TEMPLATES {
        let out = apply_honest_compile_note(template, true);
        assert_eq!(
            &out, template,
            "{tool}: content must be untouched when the on-edit hook is installed"
        );
        assert!(
            out.contains("runs automatically via hooks") || out.contains("auto-runs via hooks"),
            "{tool}: template no longer contains the claim this module rewrites — \
             update REPLACEMENTS to match the template wording"
        );
    }
}

#[test]
fn test_claims_rewritten_when_on_edit_false() {
    for (tool, template) in CLAIMING_TEMPLATES {
        let out = apply_honest_compile_note(template, false);
        assert!(
            !out.contains("runs automatically via hooks"),
            "{tool}: false claim must not survive when on-edit hook is not installed"
        );
        assert!(
            !out.contains("auto-runs via hooks"),
            "{tool}: false claim must not survive when on-edit hook is not installed"
        );
        assert!(
            out.contains("on-edit hook is not installed")
                || out.contains("on-edit hook not installed"),
            "{tool}: honest instruction must be present"
        );
    }
}
