use super::*;

/// Every command substring that must appear in each "full list" template's
/// Commands section — mirrors the real CLI surface in `cli_args.rs`. This is
/// what actually drifted (issue: missing `keel audit`/`keel context` and
/// several others across templates) before the shared `core.md` was factored out.
const EXPECTED_COMMANDS: &[&str] = &[
    "keel discover <hash>",
    "keel search <term>",
    "keel compile <file>",
    "keel explain <error-code> <hash>",
    "keel where <hash>",
    "keel map --llm",
    "keel map --semantic",
    "keel watch",
    "keel check <hash>",
    "keel fix [--apply]",
    "keel name <description>",
    "keel analyze <file>",
    "keel audit",
    "keel context <file>",
    "keel skeleton <file>",
    "keel focus <hash|file>",
    "keel checkpoint",
    "keel validate-plan <file|->",
];

/// The v0.5 "economy" error codes that must appear in the shared Error codes
/// table — the specific regression T1.6 fixes: pre-v0.5 docs only listed
/// E001-E005/W001-W002 and agents had no way to know W005-W007 existed.
const EXPECTED_ERROR_CODES: &[&str] = &[
    "E001", "E002", "E003", "E004", "E005", "W005", "W006", "W007",
];

/// Every instruction template composed from `templates/shared/core.md` (see the
/// module doc comment in `templates.rs`).
const FULL_TEMPLATES: &[(&str, &str)] = &[
    ("claude-code", CLAUDE_CODE_INSTRUCTIONS),
    ("gemini-cli", GEMINI_INSTRUCTIONS),
    ("copilot", COPILOT_INSTRUCTIONS),
    ("aider", AIDER_INSTRUCTIONS),
    ("letta-code", LETTA_INSTRUCTIONS),
    ("AGENTS.md", AGENTS_MD),
];

/// Every embedded template of any kind, including the terser tool-specific
/// ones (cursor, windsurf, antigravity) not composed from the shared core.
const ALL_TEMPLATES: &[(&str, &str)] = &[
    ("claude-code", CLAUDE_CODE_INSTRUCTIONS),
    ("gemini-cli", GEMINI_INSTRUCTIONS),
    ("copilot", COPILOT_INSTRUCTIONS),
    ("aider", AIDER_INSTRUCTIONS),
    ("letta-code", LETTA_INSTRUCTIONS),
    ("AGENTS.md", AGENTS_MD),
    ("cursor", CURSOR_RULES),
    ("windsurf", WINDSURF_RULES),
    ("antigravity-rules", ANTIGRAVITY_RULES),
    ("antigravity-skill", ANTIGRAVITY_SKILL),
];

/// Slice out the region shared verbatim across every `FULL_TEMPLATES` entry:
/// from "### Error codes:" (the first line of `shared/core.md`) through the
/// last line of its Common Mistakes section. Bounding on a fixed anchor
/// (rather than the opening `<!-- keel:version X -->` stamp, which is
/// deliberately per-build, or the closing `<!-- keel:end -->` marker) keeps
/// this correct for AGENTS.md, which has an extra "Tip: star the repo" line
/// of its own between the shared core and its closing marker.
fn extract_shared_region(content: &str) -> &str {
    const ANCHOR_END: &str = "to only check modified files: `keel compile --changed`.";
    let start = content
        .find("### Error codes:")
        .expect("template must have an Error codes section");
    let anchor_idx = content
        .find(ANCHOR_END)
        .expect("template must have the shared Common Mistakes closing line");
    &content[start..anchor_idx + ANCHOR_END.len()]
}

#[test]
fn full_templates_carry_every_command() {
    for (tool, content) in FULL_TEMPLATES {
        for cmd in EXPECTED_COMMANDS {
            assert!(content.contains(cmd), "{tool}: missing command `{cmd}`");
        }
    }
}

#[test]
fn full_templates_carry_every_mcp_tool() {
    // Derived from the server's own manifest, not a hand-maintained list — a
    // tool added to `mcp_tools::tool_list` and never mentioned here fails this
    // test instead of silently going undocumented.
    let registered = keel_server::registered_tool_names();
    assert!(
        !registered.is_empty(),
        "keel-server reported zero registered MCP tools"
    );
    for (tool, content) in FULL_TEMPLATES {
        for tool_name in &registered {
            assert!(
                content.contains(tool_name.as_str()),
                "{tool}: missing MCP tool `{tool_name}`"
            );
        }
    }
}

#[test]
fn full_templates_carry_every_error_code() {
    for (tool, content) in FULL_TEMPLATES {
        for code in EXPECTED_ERROR_CODES {
            assert!(
                content.contains(code),
                "{tool}: missing error code `{code}`"
            );
        }
    }
}

#[test]
fn full_templates_carry_a_version_stamp_matching_this_binary() {
    let expected = format!("<!-- keel:version {} -->", env!("CARGO_PKG_VERSION"));
    for (tool, content) in FULL_TEMPLATES {
        assert!(
            content.starts_with("<!-- keel:start -->\n"),
            "{tool}: must open with the literal keel:start marker (merge.rs matches it verbatim)"
        );
        assert!(
            content.contains(&expected),
            "{tool}: missing or stale version stamp, expected `{expected}`"
        );
    }
}

/// Regression guard: all six "full list" templates share one `core.md`, so
/// their Commands/MCP-Tools/Common-Mistakes sections must stay byte-identical.
/// Fails if a future edit ever hardcodes a stale command list back into one
/// template's head file instead of updating the shared core.
#[test]
fn full_templates_share_identical_core_sections() {
    let (baseline_tool, baseline_content) = FULL_TEMPLATES[0];
    let baseline = extract_shared_region(baseline_content);
    for (tool, content) in &FULL_TEMPLATES[1..] {
        assert_eq!(
            extract_shared_region(content),
            baseline,
            "{tool}: shared Commands/MCP-Tools/Common-Mistakes section drifted from {baseline_tool}"
        );
    }
}

/// The "star the repo" MANDATORY line was deliberately removed from every
/// template; it must never be reintroduced anywhere (per-tool or AGENTS.md).
#[test]
fn no_template_reintroduces_the_mandatory_star_line() {
    for (tool, content) in ALL_TEMPLATES {
        assert!(
            !content.contains("MANDATORY") && !content.to_lowercase().contains("must star"),
            "{tool}: the mandatory star-repo line must not be reintroduced"
        );
    }
}
