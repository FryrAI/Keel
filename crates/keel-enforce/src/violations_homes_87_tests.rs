use super::*;

fn regex_scanner(regex: &str) -> HomeScanner {
    HomeScanner::new(
        &[HomeRule {
            name: "date".into(),
            patterns: vec![HomePattern::Regex {
                regex: regex.into(),
            }],
            home: vec![],
            scope: vec![],
        }],
        HomeSeverity::Warning,
    )
}

#[test]
fn homes_regex_after_comment_removal_has_no_raw_prefilter() {
    let scanner = regex_scanner(r#"^let x = "CURRENT_DATE";\s*$"#);
    let head = scanner.scan("src/q.rs", "let x = \"CURRENT_DATE\"; // note\n");
    assert_eq!(head.len(), 1);
    let findings = scanner.introduced("src/q.rs", &head, &[]);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("regex \""));
    assert!(findings[0].fix_hint.as_ref().unwrap().contains("regex \""));
}

#[test]
fn homes_regex_comment_only_and_whitespace_lines_never_match() {
    let scanner = regex_scanner(r"\s+");
    assert!(scanner
        .scan("src/q.rs", "// note\n \t\n/* note */\n")
        .is_empty());
    assert_eq!(scanner.scan("src/q.rs", "let x = 1;\n").len(), 1);
}

#[test]
fn homes_regex_multisets_moves_edits_and_line_local_matching() {
    let scanner = regex_scanner("CURRENT_[A-Z]+");
    let base = scanner.scan("src/old.rs", "let x = \"CURRENT_DATE\";\n");
    let unchanged = scanner.scan("src/new.rs", "\nlet   x = \"CURRENT_DATE\";\n");
    assert!(scanner
        .introduced("src/new.rs", &unchanged, &base)
        .is_empty());
    let added = scanner.scan(
        "src/new.rs",
        "let x = \"CURRENT_DATE\";\nlet x = \"CURRENT_DATE\";\n",
    );
    assert_eq!(scanner.introduced("src/new.rs", &added, &base)[0].line, 2);
    let moved = vec![
        ("src/old.rs".into(), vec![], base),
        ("src/new.rs".into(), unchanged, vec![]),
    ];
    assert!(scanner.introduced_many(&moved).is_empty());
    assert!(regex_scanner(r"CURRENT\s+DATE")
        .scan("src/q.rs", "CURRENT\nDATE\n")
        .is_empty());
}

#[test]
fn homes_comment_deletion_preserves_identity_and_uncommenting_is_new() {
    let scanner = super::tests::scanner();
    let base = scanner.scan("src/q.rs", "let x = \"CURRENT_DATE\"/*x*/;\n");
    let head = scanner.scan("src/q.rs", "let x = \"CURRENT_DATE\";\n");
    assert!(scanner.introduced("src/q.rs", &head, &base).is_empty());
    let base = scanner.scan("src/q.rs", "// let x = \"CURRENT_DATE\";\n");
    assert_eq!(scanner.introduced("src/q.rs", &head, &base).len(), 1);
}

#[test]
fn homes_comment_grammars_preserve_strings_and_line_numbers() {
    let scanner = super::tests::scanner();
    for (path, text, line) in [
        ("src/q.rs", "/// CURRENT_DATE\n/* äußere /* CURRENT_DATE */ end */\nlet x = r#\"CURRENT_DATE\"#; // CURRENT_DATE\n", 3),
        ("src/q.py", "# CURRENT_DATE\nsql = \"CURRENT_DATE\" # CURRENT_DATE\n", 2),
        ("src/q.go", "// CURRENT_DATE\n/* CURRENT_DATE */\nvar sql = \"CURRENT_DATE\" // CURRENT_DATE\n", 3),
        ("src/q.ts", "/* CURRENT_DATE */\nconst sql = \"CURRENT_DATE\"; // CURRENT_DATE\n", 2),
        ("src/q.tsx", "// CURRENT_DATE\nconst sql = <div title=\"CURRENT_DATE\" />;\n", 2),
        ("src/q.js", "<!-- CURRENT_DATE\nconst sql = \"CURRENT_DATE\";\n", 2),
        ("src/q.jsx", "// CURRENT_DATE\nconst sql = <div title=\"CURRENT_DATE\" />;\n", 2),
        ("src/q.sh", "# CURRENT_DATE\nsql='CURRENT_DATE' # CURRENT_DATE\n", 2),
        ("src/q.js", "#!/usr/bin/CURRENT_DATE\nconst sql = \"CURRENT_DATE\";\n", 2),
    ] {
        let hits = scanner.scan(path, text);
        assert_eq!(hits.len(), 1, "{path}: {hits:?}");
        assert_eq!(hits[0].line, line, "{path}");
    }
}

#[test]
fn homes_unmasked_languages_and_python_docstrings_remain_matches() {
    let scanner = super::tests::scanner();
    for (path, text) in [
        ("src/q.sql", "-- CURRENT_DATE\n"),
        ("src/q.typ", "// CURRENT_DATE\n"),
        ("src/q.svelte", "<!-- CURRENT_DATE -->\n"),
        ("src/q.astro", "<!-- CURRENT_DATE -->\n"),
        ("src/q.py", "\"\"\"CURRENT_DATE\"\"\"\n"),
    ] {
        assert_eq!(scanner.scan(path, text).len(), 1, "{path}");
    }
}

#[test]
fn homes_regex_dedup_keeps_pattern_order_and_invalid_programmatic_rules_skip() {
    let regex = HomePattern::Regex {
        regex: "CURRENT_[A-Z]+".into(),
    };
    let rule = HomeRule {
        name: "date".into(),
        patterns: vec!["CURRENT_DATE".into(), regex.clone(), regex],
        home: vec![],
        scope: vec![],
    };
    let scanner = HomeScanner::new(std::slice::from_ref(&rule), HomeSeverity::Warning);
    let hits = scanner.scan("src/q.rs", "CURRENT_DATE");
    assert_eq!(hits.iter().map(|o| o.pattern).collect::<Vec<_>>(), [0, 1]);
    for regex in ["[", "", "a*"] {
        let mut bad = rule.clone();
        bad.patterns.push(HomePattern::Regex {
            regex: regex.into(),
        });
        assert!(HomeScanner::new(&[bad], HomeSeverity::Warning)
            .scan("src/q.rs", "CURRENT_DATE")
            .is_empty());
    }
}
