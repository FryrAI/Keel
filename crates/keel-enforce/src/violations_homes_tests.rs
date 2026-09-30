use super::*;

/// Prepare the literal rules shared with the comment and regex regressions.
pub(super) fn scanner() -> HomeScanner {
    HomeScanner::new(
        &[
            HomeRule {
                name: "civil-day".into(),
                patterns: vec!["date_naive()".into(), "CURRENT_DATE".into()],
                home: vec!["src/time.rs".into()],
                scope: vec!["src".into()],
            },
            HomeRule {
                name: "utc-day".into(),
                patterns: vec!["date_naive()".into()],
                home: vec![],
                scope: vec!["src".into()],
            },
        ],
        HomeSeverity::Warning,
    )
}

#[test]
fn homes_scan_code_raw_strings_triple_quotes_excludes_comments() {
    let scanner = scanner();
    let text = "value.date_naive();\nlet sql = r#\"CURRENT_DATE\"#;\n// CURRENT_DATE\n";
    let hits = scanner.scan("src/lib.rs", text);
    assert_eq!(hits.len(), 3);
    assert_eq!(
        scanner
            .scan("src/query.py", "sql = \"\"\"\nCURRENT_DATE\n\"\"\"\n")
            .len(),
        1
    );
    assert!(scanner.scan("src/lib.rs", "current_date").is_empty());
    assert!(scanner.scan("src/data.json", "CURRENT_DATE").is_empty());
}

#[test]
fn homes_group_rules_and_patterns_per_line_but_preserve_identical_lines() {
    let scanner = scanner();
    let head = scanner.scan(
        "src/lib.rs",
        "date_naive() CURRENT_DATE\ndate_naive() CURRENT_DATE\n",
    );
    let violations = scanner.introduced("src/lib.rs", &head, &[]);
    assert_eq!(violations.len(), 2);
    assert_eq!(violations[1].line, 2);
    assert!(violations.iter().all(|v| v.hash.is_empty()));
    assert!(violations[0].message.contains("civil-day"));
    assert!(violations[0].message.contains("utc-day"));
    assert!(violations[0].message.contains("CURRENT_DATE"));
    assert!(violations[0].message.contains("src/time.rs"));
}

#[test]
fn homes_multiset_ignores_line_moves_and_whitespace_but_reports_surplus_and_edits() {
    let scanner = scanner();
    let base = scanner.scan("src/lib.rs", "let x = CURRENT_DATE;\n");
    let head = scanner.scan("src/lib.rs", "\n\tlet   x =  CURRENT_DATE;\n");
    assert!(scanner.introduced("src/lib.rs", &head, &base).is_empty());
    let head = scanner.scan(
        "src/lib.rs",
        "let x = CURRENT_DATE;\nlet x = CURRENT_DATE;\n",
    );
    let violations = scanner.introduced("src/lib.rs", &head, &base);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].line, 2);
    let head = scanner.scan("src/lib.rs", "let y = CURRENT_DATE;\n");
    assert_eq!(scanner.introduced("src/lib.rs", &head, &base).len(), 1);
}

#[test]
fn homes_globs_match_directory_prefixes_without_crossing_separators() {
    let rule = HomeRule {
        name: "x".into(),
        patterns: vec!["CURRENT_DATE".into()],
        home: vec!["crates/core/src/time".into()],
        scope: vec!["crates/*/src".into()],
    };
    let scanner = HomeScanner::new(&[rule], HomeSeverity::Error);
    assert_eq!(
        scanner
            .scan("crates/core/src/deep/lib.rs", "CURRENT_DATE")
            .len(),
        1
    );
    assert!(scanner
        .scan("crates/core/src/time/lib.rs", "CURRENT_DATE")
        .is_empty());
    assert!(scanner
        .scan("crates/deep/core/src/lib.rs", "CURRENT_DATE")
        .is_empty());
    let violations = scanner.introduced(
        "crates/core/src/lib.rs",
        &scanner.scan("crates/core/src/lib.rs", "CURRENT_DATE"),
        &[],
    );
    assert_eq!(violations[0].code, "E007");
    assert_eq!(violations[0].severity, "ERROR");
    assert!(violations[0].fix_hint.is_some());
}

#[test]
fn homes_rename_eligibility_is_applied_to_both_sides() {
    let scanner = scanner();
    let text = "CURRENT_DATE";
    let head = scanner.scan("src/new.rs", text);
    let home_base = scanner.scan("src/time.rs", text);
    let scope_base = scanner.scan("outside/lib.rs", text);
    assert_eq!(scanner.introduced("src/new.rs", &head, &home_base).len(), 1);
    assert_eq!(
        scanner.introduced("src/new.rs", &head, &scope_base).len(),
        1
    );
    let eligible_base = scanner.scan("src/old.rs", text);
    assert!(scanner
        .introduced("src/new.rs", &head, &eligible_base)
        .is_empty());
}

#[test]
fn homes_duplicate_patterns_and_multiple_substrings_count_once_per_line() {
    let scanner = HomeScanner::new(
        &[HomeRule {
            name: "x".into(),
            patterns: vec!["CURRENT_DATE".into(), "CURRENT_DATE".into()],
            home: vec![],
            scope: vec![],
        }],
        HomeSeverity::Warning,
    );
    let head = scanner.scan("lib.rs", "CURRENT_DATE CURRENT_DATE");
    assert_eq!(head.len(), 1);
    assert_eq!(scanner.introduced("lib.rs", &head, &[]).len(), 1);
}
