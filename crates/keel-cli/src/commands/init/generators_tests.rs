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
