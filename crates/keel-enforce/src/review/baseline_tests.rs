//! Baseline-diff tests: what a PR *introduced*, and nothing it inherited.

use std::collections::HashSet;

use keel_core::sqlite::SqliteGraphStore;
use keel_parsers::resolver::{Definition, FileIndex};

use super::*;
use crate::test_fixtures::{definition, file_index};

fn store() -> SqliteGraphStore {
    SqliteGraphStore::in_memory().unwrap()
}

fn scan(base: Vec<FileIndex>, head: Vec<FileIndex>) -> DiffScan {
    DiffScan {
        changes: Vec::new(),
        unanalyzed: Vec::new(),
        diff_files: HashSet::new(),
        files_analyzed: base.len().max(head.len()),
        base_indices: base,
        head_indices: head,
        renames: BTreeMap::new(),
    }
}

/// A public, undocumented function — one E003 per side it appears on.
fn undocumented(name: &str, file: &str, line: u32, body: &str) -> Definition {
    Definition {
        line_start: line,
        line_end: line + 10,
        body_text: body.to_string(),
        ..definition(name, file, true)
    }
}

/// A definition whose extent alone decides the file's size.
fn spanning(name: &str, file: &str, line_end: u32) -> Definition {
    Definition {
        docstring: Some("Documented.".into()),
        line_start: 1,
        line_end,
        ..definition(name, file, true)
    }
}

#[test]
fn a_reformat_of_a_file_full_of_violations_introduces_none() {
    // Same two undocumented functions, shifted down 40 lines and reindented.
    let base = vec![file_index(
        "src/lib.rs",
        vec![
            undocumented("alpha", "src/lib.rs", 10, "let x = 1;"),
            undocumented("beta", "src/lib.rs", 30, "let y = 2;"),
        ],
    )];
    let head = vec![file_index(
        "src/lib.rs",
        vec![
            undocumented("alpha", "src/lib.rs", 50, "    let x = 1;"),
            undocumented("beta", "src/lib.rs", 70, "    let y = 2;"),
        ],
    )];

    let out = diff(&store(), &scan(base, head), &EnforceConfig::default());
    assert!(out.new_violations.is_empty(), "{:?}", out.new_violations);
    assert_eq!(out.pre_existing, 2);
}

#[test]
fn only_the_symbol_the_pr_added_is_reported() {
    let base = vec![file_index(
        "src/lib.rs",
        vec![undocumented("alpha", "src/lib.rs", 10, "let x = 1;")],
    )];
    let head = vec![file_index(
        "src/lib.rs",
        vec![
            undocumented("alpha", "src/lib.rs", 10, "let x = 1;"),
            undocumented("fresh", "src/lib.rs", 40, "let z = 3;"),
        ],
    )];

    let out = diff(&store(), &scan(base, head), &EnforceConfig::default());
    assert_eq!(out.new_violations.len(), 1);
    assert_eq!(out.new_violations[0].code, "E003");
    assert!(out.new_violations[0].message.contains("fresh"));
    assert_eq!(out.pre_existing, 1);
}

#[test]
fn a_body_edit_does_not_resurface_the_functions_existing_findings() {
    // The one case an AST-hash identity would get wrong: the contract holds,
    // the body changed, and the function still has no docstring.
    let base = vec![file_index(
        "src/lib.rs",
        vec![undocumented("alpha", "src/lib.rs", 10, "let x = 1;")],
    )];
    let head = vec![file_index(
        "src/lib.rs",
        vec![undocumented(
            "alpha",
            "src/lib.rs",
            10,
            "let x = compute(1) + 2;",
        )],
    )];

    let out = diff(&store(), &scan(base, head), &EnforceConfig::default());
    assert!(out.new_violations.is_empty(), "{:?}", out.new_violations);
}

#[test]
fn renaming_a_file_does_not_reintroduce_its_findings() {
    let base = vec![file_index(
        "src/old.rs",
        vec![undocumented("alpha", "src/old.rs", 10, "let x = 1;")],
    )];
    let head = vec![file_index(
        "src/new.rs",
        vec![undocumented("alpha", "src/new.rs", 10, "let x = 1;")],
    )];
    let mut s = scan(base, head);
    s.renames.insert("src/new.rs".into(), "src/old.rs".into());

    let out = diff(&store(), &s, &EnforceConfig::default());
    assert!(out.new_violations.is_empty(), "{:?}", out.new_violations);
}

#[test]
fn a_file_pushed_past_the_budget_reports_exactly_one_size_finding() {
    let cfg = EnforceConfig {
        max_file_lines: 400,
        ..EnforceConfig::default()
    };
    let base = vec![file_index(
        "src/page.ts",
        vec![spanning("render", "src/page.ts", 380)],
    )];
    let head = vec![file_index(
        "src/page.ts",
        vec![spanning("render", "src/page.ts", 430)],
    )];

    let out = diff(&store(), &scan(base, head), &cfg);
    let sizes: Vec<&Violation> = out
        .new_violations
        .iter()
        .filter(|v| v.code == "W007")
        .collect();
    assert_eq!(sizes.len(), 1, "{:?}", out.new_violations);
    assert_eq!(sizes[0].file, "src/page.ts");
}

#[test]
fn a_file_already_over_the_budget_is_not_this_prs_problem() {
    let cfg = EnforceConfig {
        max_file_lines: 400,
        ..EnforceConfig::default()
    };
    // Already 900 lines before the PR; the PR grew it to 950. W007 against the
    // stored graph would fire; baseline-relative, it is inherited.
    let base = vec![file_index(
        "src/page.ts",
        vec![spanning("render", "src/page.ts", 900)],
    )];
    let head = vec![file_index(
        "src/page.ts",
        vec![spanning("render", "src/page.ts", 950)],
    )];

    let out = diff(&store(), &scan(base, head), &cfg);
    assert!(
        out.new_violations.iter().all(|v| v.code != "W007"),
        "{:?}",
        out.new_violations
    );
    assert!(out.pre_existing >= 1);
}

#[test]
fn a_disabled_check_cannot_produce_a_new_finding() {
    let cfg = EnforceConfig {
        docstrings: false,
        ..EnforceConfig::default()
    };
    let base = vec![file_index("src/lib.rs", vec![])];
    let head = vec![file_index(
        "src/lib.rs",
        vec![undocumented("fresh", "src/lib.rs", 40, "let z = 3;")],
    )];

    let out = diff(&store(), &scan(base, head), &cfg);
    assert!(out.new_violations.is_empty(), "{:?}", out.new_violations);
}

#[test]
fn only_diffable_codes_ever_reach_the_pr_surface() {
    let base = vec![file_index("src/lib.rs", vec![])];
    let head = vec![file_index(
        "src/lib.rs",
        vec![undocumented("fresh", "src/lib.rs", 40, "let z = 3;")],
    )];
    let out = diff(&store(), &scan(base, head), &EnforceConfig::default());
    assert!(out
        .new_violations
        .iter()
        .all(|v| DIFFABLE_CODES.contains(&v.code.as_str())));
    // E001/E004/E005 are head-only by construction: they need cross-file
    // reference resolution the base blobs never got.
    for code in ["E001", "E004", "E005", "W001", "W002", "W009"] {
        assert!(
            !DIFFABLE_CODES.contains(&code),
            "{code} must stay off the baseline surface"
        );
    }
}

#[test]
fn gating_is_off_until_a_code_is_named() {
    let v = Violation {
        code: "W007".into(),
        severity: "WARNING".into(),
        category: "oversized_file".into(),
        message: "big".into(),
        file: "src/page.ts".into(),
        line: 1,
        hash: String::new(),
        confidence: 0.8,
        resolution_tier: "heuristic".into(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    };
    assert!(gate_hits(std::slice::from_ref(&v), &[]).is_empty());
    assert!(gate_hits(std::slice::from_ref(&v), &["E003".to_string()]).is_empty());
    assert_eq!(
        gate_hits(std::slice::from_ref(&v), &["W007".to_string()]).len(),
        1
    );
}
