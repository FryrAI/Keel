use super::*;

use keel_enforce::validate_plan::PlanValidationResult;
use keel_enforce::validate_plan_findings::PlanFinding;

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

/// The scaffolded GitHub Actions workflow and the maintained composite action
/// must be the same recipe (T2.3). The scaffold used to `curl install.sh` and
/// run `keel map --json --strict`, so a user who followed keel's own setup
/// never got keel's own annotations or its PR comment.
#[test]
fn the_github_actions_scaffold_calls_the_maintained_action() {
    assert!(
        GITHUB_ACTIONS.contains("uses: FryrAI/Keel/.github/actions/keel@v0"),
        "the scaffold must call the maintained composite action"
    );
    assert!(
        GITHUB_ACTIONS.contains("fetch-depth: 0"),
        "keel review parses the base side straight out of git — a shallow clone has none"
    );
    assert!(
        GITHUB_ACTIONS.contains("pull_request") && GITHUB_ACTIONS.contains("push"),
        "the scaffold must cover both the review (pull_request) and compile (push) paths"
    );
    assert!(
        GITHUB_ACTIONS.contains("pull-requests: write"),
        "the sticky review comment needs write permission on pull requests"
    );
    for forbidden in ["install.sh", "map --json --strict"] {
        assert!(
            !GITHUB_ACTIONS.contains(forbidden),
            "the scaffold must not keep its own divergent recipe (`{forbidden}`)"
        );
    }
}

/// The exact `grep -E` pattern `.keel/hooks/plan-check.sh` filters `keel
/// validate-plan --llm` output with. Only the lines it matches ever reach the
/// model, so these prefixes are a contract between the hook and the LLM
/// formatter — not a formatting detail.
const PLAN_HOOK_GREP: &str = r"'^(P00[12] |  at: |  fix: )'";

/// The hook's pattern, transcribed to Rust.
fn plan_hook_matches(line: &str) -> bool {
    line.starts_with("P001 ")
        || line.starts_with("P002 ")
        || line.starts_with("  at: ")
        || line.starts_with("  fix: ")
}

fn plan_finding(code: &str, symbol: &str, hash: &str, line: u32) -> PlanFinding {
    PlanFinding {
        code: code.to_string(),
        severity: "WARNING".to_string(),
        category: "unknown_symbol".to_string(),
        symbol: symbol.to_string(),
        message: "the plan calls a symbol the graph does not have".to_string(),
        hash: hash.to_string(),
        file: if hash.is_empty() {
            String::new()
        } else {
            "src/lib.rs".to_string()
        },
        line,
        claimed: format!("{symbol}(a, b)"),
        actual: None,
        fix_hint: "check the name against `keel search`".to_string(),
        confidence: 0.9,
        downgraded: false,
    }
}

/// Renders a P001 and a P002 finding through the real LLM formatter and asserts
/// the hook's grep still selects exactly the finding lines. Without this, a
/// formatter reflow would silently empty the plan hook (no findings shown reads
/// as a clean plan — the worst possible failure mode).
#[test]
fn plan_check_hook_grep_matches_the_llm_formatters_finding_lines() {
    assert!(
        PLAN_CHECK_HOOK.contains(PLAN_HOOK_GREP),
        "the hook's grep pattern changed; update PLAN_HOOK_GREP and plan_hook_matches together"
    );

    let result = PlanValidationResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "validate-plan".to_string(),
        actions: Vec::new(),
        symbols_detected: 2,
        files_detected: vec!["src/lib.rs".to_string()],
        unrecognized: false,
        findings: vec![
            plan_finding("P001", "make_widget", "", 0),
            plan_finding("P002", "execute", "abc12345678", 42),
        ],
    };

    let rendered = keel_output::llm::validate_plan::format_validate_plan(&result);
    let matched: Vec<&str> = rendered.lines().filter(|l| plan_hook_matches(l)).collect();

    assert!(
        matched
            .iter()
            .any(|l| l.starts_with("P001 WARNING make_widget")),
        "hook must still see the P001 line: {rendered}"
    );
    assert!(
        matched
            .iter()
            .any(|l| l.starts_with("P002 WARNING execute")),
        "hook must still see the P002 line: {rendered}"
    );
    assert!(
        matched.iter().any(|l| l.starts_with("  at: src/lib.rs:42")),
        "hook must still see the location line: {rendered}"
    );
    assert_eq!(
        matched.iter().filter(|l| l.starts_with("  fix: ")).count(),
        2,
        "hook must still see one fix line per finding: {rendered}"
    );
    assert!(
        !matched.iter().any(|l| l.starts_with("VALIDATE-PLAN")),
        "the summary line must stay filtered out: {rendered}"
    );
}
