use std::path::{Path, PathBuf};

use super::*;
use crate::commands::init::HookSelection;

type GeneratorFn = fn(&Path, &HookSelection) -> Vec<(PathBuf, String)>;

/// Every generator whose instruction file claims automatic on-edit compiles,
/// with the generated file to inspect.
const CASES: &[(&str, GeneratorFn, &str)] = &[
    ("claude-code", generate_claude_code, "CLAUDE.md"),
    ("gemini", generate_gemini_cli, "GEMINI.md"),
    ("letta", generate_letta_code, "LETTA.md"),
    ("cursor", generate_cursor, "keel.mdc"),
    ("windsurf", generate_windsurf, ".windsurfrules"),
];

fn hooks_with_on_edit(on_edit: bool) -> HookSelection {
    HookSelection {
        session_start: true,
        pre_commit: true,
        pre_commit_audit: true,
        on_edit,
    }
}

/// Find the generated file whose path ends with `name` and return its content.
fn content_for<'a>(files: &'a [(PathBuf, String)], name: &str) -> &'a str {
    &files
        .iter()
        .find(|(p, _)| p.ends_with(name))
        .unwrap_or_else(|| panic!("no generated file ending with {name}"))
        .1
}

#[test]
fn test_generated_instructions_match_hook_selection() {
    let root = tempfile::tempdir().unwrap();

    for (tool, generate, file) in CASES {
        let off = generate(root.path(), &hooks_with_on_edit(false));
        let off_content = content_for(&off, file);
        assert!(
            !off_content.contains("runs automatically via hooks")
                && !off_content.contains("auto-runs via hooks"),
            "{tool}: on-edit off must not claim automatic compilation"
        );
        assert!(
            off_content.contains("on-edit hook is not installed")
                || off_content.contains("on-edit hook not installed"),
            "{tool}: on-edit off must carry the honest manual-compile instruction"
        );

        let on = generate(root.path(), &hooks_with_on_edit(true));
        let on_content = content_for(&on, file);
        assert!(
            on_content.contains("runs automatically via hooks")
                || on_content.contains("auto-runs via hooks"),
            "{tool}: on-edit on should keep the (now accurate) claim"
        );
    }
}

/// T2.6: Claude Code — and only Claude Code — gets the `ExitPlanMode`
/// PreToolUse hook. Other tools' payload shapes are unverified, and a hook that
/// silently no-ops is worse than none (no findings reads as a clean plan).
#[test]
fn test_claude_code_gets_exit_plan_mode_hook_and_others_do_not() {
    let root = tempfile::tempdir().unwrap();

    let files = generate_claude_code(root.path(), &hooks_with_on_edit(false));
    let settings: serde_json::Value =
        serde_json::from_str(content_for(&files, "settings.json")).unwrap();
    let pre = settings["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse must be present for Claude Code");
    let entry = pre
        .iter()
        .find(|e| e["matcher"] == "ExitPlanMode")
        .expect("matcher must be ExitPlanMode");
    assert_eq!(entry["hooks"][0]["type"], "command");
    assert_eq!(entry["hooks"][0]["command"], ".keel/hooks/plan-check.sh");

    let others: &[(&str, GeneratorFn, &str)] = &[
        ("gemini", generate_gemini_cli, "settings.json"),
        ("letta", generate_letta_code, "settings.json"),
        ("cursor", generate_cursor, "hooks.json"),
        ("windsurf", generate_windsurf, "hooks.json"),
    ];
    for (tool, generate, file) in others {
        let files = generate(root.path(), &hooks_with_on_edit(false));
        assert!(
            !content_for(&files, file).contains("ExitPlanMode"),
            "{tool} must not scaffold an ExitPlanMode hook"
        );
    }
}

/// The plan-check hook is advisory: it must never exit non-zero on the default
/// path, and it must document both the bypass and the opt-in blocking mode.
#[test]
fn test_plan_check_hook_is_advisory_by_default() {
    let script = templates::PLAN_CHECK_HOOK;
    assert!(script.contains("validate-plan --llm -") || script.contains("validate-plan --llm"));
    assert!(
        script.contains(".tool_input.plan"),
        "must read the plan payload"
    );
    assert!(
        script.contains("KEEL_PLAN_HOOK=0"),
        "must document the bypass"
    );
    assert!(
        script.contains("KEEL_PLAN_STRICT"),
        "must document opt-in blocking"
    );
    // The only non-zero exit statement sits inside the opt-in strict branch.
    let code_lines: Vec<(usize, &str)> = script
        .lines()
        .enumerate()
        .map(|(i, l)| (i, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let strict_gate = code_lines
        .iter()
        .position(|(_, l)| l.starts_with("if [ \"$STRICT\" = \"1\" ]"))
        .expect("strict branch must exist");
    let blocking: Vec<usize> = code_lines
        .iter()
        .enumerate()
        .filter(|(_, (_, l))| l.starts_with("exit 2"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(blocking.len(), 1, "exactly one blocking exit expected");
    assert!(
        blocking[0] > strict_gate,
        "exit 2 must only be reachable from the opt-in strict branch"
    );
    assert!(script.trim_end().ends_with("exit 0"));
}

/// The single shared `.keel/hooks/post-edit.sh` cannot infer which tool
/// invoked it, so each tool's own on-edit hook JSON must pass its client
/// name as an explicit argument (T1.6: env-var detection does not reliably
/// survive into the hook's subprocess).
#[test]
fn test_on_edit_hook_command_passes_client_argument() {
    let root = tempfile::tempdir().unwrap();
    let cases: &[(&str, GeneratorFn, &str, &str)] = &[
        (
            "claude-code",
            generate_claude_code,
            "settings.json",
            "claude-code",
        ),
        ("gemini", generate_gemini_cli, "settings.json", "gemini-cli"),
        ("letta", generate_letta_code, "settings.json", "letta-code"),
        ("cursor", generate_cursor, "hooks.json", "cursor"),
        ("windsurf", generate_windsurf, "hooks.json", "windsurf"),
    ];
    for (tool, generate, file, client) in cases {
        let files = generate(root.path(), &hooks_with_on_edit(true));
        let content = content_for(&files, file);
        let expected = format!(".keel/hooks/post-edit.sh {client}");
        assert!(
            content.contains(&expected),
            "{tool}: on-edit hook command must pass `{expected}`, got: {content}"
        );
    }
}
