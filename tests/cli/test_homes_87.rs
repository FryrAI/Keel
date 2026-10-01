//! Issue #87 part A regressions and explicit compatibility controls.

use super::*;
use keel_core::store::GraphStore;

fn patterns(root: &Path, patterns: Value) {
    let path = root.join(".keel/keel.json");
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    cfg["homes"][0]["patterns"] = patterns;
    fs::write(path, cfg.to_string()).unwrap();
}

fn commit_paths(root: &Path, paths: &[&str]) {
    let mut args = vec!["add"];
    args.extend_from_slice(paths);
    git(root, &args);
    git(root, &["commit", "-q", "-m", "fixture source"]);
}

#[test]
fn homes_regex_cli_compile_review_and_line_local_matching() {
    let dir = fixture(true);
    patterns(dir.path(), json!([{"regex": "CURRENT_[A-Z]+"}]));
    write(
        dir.path(),
        "src/q.rs",
        "const SQL: &str = \"CURRENT_TIME\";\n",
    );
    let out = keel(dir.path(), &["compile", "src/q.rs", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let hits = violations_with_code(&parse(&out), "W011");
    assert_eq!(hits.len(), 1);
    assert!(hits[0]["message"]
        .as_str()
        .unwrap()
        .contains("regex \"CURRENT_[A-Z]+\""));
    git(dir.path(), &["add", "src/q.rs"]);
    let out = review(dir.path(), "HEAD", false);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(review_hits(&out, "W011").len(), 1);
    patterns(dir.path(), json!([{"regex": "CURRENT\\s+TIME"}]));
    write(
        dir.path(),
        "src/q.rs",
        "const SQL: &str = \"CURRENT\nTIME\";\n",
    );
    assert!(violations_with_code(&compile_json(dir.path(), "src/q.rs"), "W011").is_empty());
    assert!(review_hits(&review(dir.path(), "HEAD", false), "W011").is_empty());
}

#[test]
fn homes_regex_anchored_cli_matches_after_trailing_comment_removal() {
    let dir = fixture(true);
    patterns(
        dir.path(),
        json!([{"regex": "^let x = \"CURRENT_DATE\";\\s*$"}]),
    );
    write(
        dir.path(),
        "src/q.rs",
        "let x = \"CURRENT_DATE\"; // note\n",
    );
    assert_eq!(
        violations_with_code(&compile_json(dir.path(), "src/q.rs"), "W011").len(),
        1
    );
    git(dir.path(), &["add", "src/q.rs"]);
    assert_eq!(
        review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
        1
    );
}

#[test]
fn homes_comment_only_regex_cli_has_no_findings() {
    let dir = fixture(true);
    patterns(dir.path(), json!([{"regex": "\\s+"}]));
    write(dir.path(), "src/q.rs", "// note\n/* note */\n \t\n");
    assert!(violations_with_code(&compile_json(dir.path(), "src/q.rs"), "W011").is_empty());
    git(dir.path(), &["add", "src/q.rs"]);
    assert!(review_hits(&review(dir.path(), "HEAD", false), "W011").is_empty());
    // Positive control ensures a dropped rule cannot make this test vacuously pass.
    write(dir.path(), "src/q.rs", "let x = 1;\n");
    assert_eq!(
        violations_with_code(&compile_json(dir.path(), "src/q.rs"), "W011").len(),
        1
    );
    assert_eq!(
        review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
        1
    );
}

#[test]
fn homes_comments_cli_compile_review_languages_with_string_controls() {
    for (path, comments, code) in [
        (
            "src/q.rs",
            "/// CURRENT_DATE\n/* nested /* CURRENT_DATE */ end */\n",
            "const SQL: &str = r#\"CURRENT_DATE\"#; // CURRENT_DATE\n",
        ),
        (
            "src/q.py",
            "# CURRENT_DATE\n",
            "sql = \"CURRENT_DATE\" # CURRENT_DATE\n",
        ),
        (
            "src/q.go",
            "// CURRENT_DATE\n",
            "package q\nvar sql = \"CURRENT_DATE\" // CURRENT_DATE\n",
        ),
        (
            "src/q.ts",
            "/* CURRENT_DATE */\n",
            "const sql = \"CURRENT_DATE\"; // CURRENT_DATE\n",
        ),
        (
            "src/q.js",
            "#!/usr/bin/CURRENT_DATE\n<!-- CURRENT_DATE\n",
            "const sql = \"CURRENT_DATE\"; // CURRENT_DATE\n",
        ),
        (
            "src/q.sh",
            "# CURRENT_DATE\n",
            "sql='CURRENT_DATE' # CURRENT_DATE\n",
        ),
    ] {
        let dir = fixture(true);
        write(dir.path(), path, comments);
        assert!(
            violations_with_code(&compile_json(dir.path(), path), "W011").is_empty(),
            "{path}"
        );
        git(dir.path(), &["add", path]);
        assert!(
            review_hits(&review(dir.path(), "HEAD", false), "W011").is_empty(),
            "{path}"
        );
        write(dir.path(), path, &format!("{comments}{code}"));
        assert_eq!(
            violations_with_code(&compile_json(dir.path(), path), "W011").len(),
            1,
            "{path}"
        );
        assert_eq!(
            review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
            1,
            "{path}"
        );
    }
}

#[test]
fn homes_comment_removal_cli_preserves_base_identity() {
    let dir = fixture(true);
    write(dir.path(), "src/q.rs", "let x = \"CURRENT_DATE\"/*x*/;\n");
    commit_paths(dir.path(), &["src/q.rs"]);
    write(dir.path(), "src/q.rs", "let x = \"CURRENT_DATE\";\n");
    assert!(violations_with_code(&compile_json(dir.path(), "src/q.rs"), "W011").is_empty());
    assert!(review_hits(&review(dir.path(), "HEAD", false), "W011").is_empty());
}

#[test]
fn homes_uncommenting_cli_introduces_a_finding() {
    let dir = fixture(true);
    write(dir.path(), "src/q.rs", "// let x = \"CURRENT_DATE\";\n");
    commit_paths(dir.path(), &["src/q.rs"]);
    write(dir.path(), "src/q.rs", "let x = \"CURRENT_DATE\";\n");
    assert_eq!(
        violations_with_code(&compile_json(dir.path(), "src/q.rs"), "W011").len(),
        1
    );
    assert_eq!(
        review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
        1
    );
}

#[test]
fn homes_staged_rename_compile_changed_and_destination_are_silent() {
    let dir = fixture(true);
    git(dir.path(), &["mv", "src/lib.rs", "src/new.rs"]);
    for args in [
        vec!["compile", "--changed", "--json"],
        vec!["compile", "src/new.rs", "--json"],
    ] {
        let out = keel(dir.path(), &args);
        assert_eq!(out.status.code(), Some(0));
        assert!(violations_with_code(&parse(&out), "W011").is_empty());
    }
    // Control: review already recognized staged renames before #87.
    assert!(review_hits(&review(dir.path(), "HEAD", false), "W011").is_empty());
}

#[test]
fn homes_rename_with_added_line_consumes_base_once_even_without_default_renames() {
    for default_renames in ["true", "false"] {
        let dir = fixture(true);
        let padding = "const PAD: i32 = 0;\n".repeat(20);
        write(dir.path(), "src/lib.rs", &format!("{BASE}{padding}"));
        commit_paths(dir.path(), &["src/lib.rs"]);
        git(dir.path(), &["config", "diff.renames", default_renames]);
        git(dir.path(), &["mv", "src/lib.rs", "src/new.rs"]);
        write(dir.path(), "src/new.rs", &format!("{ADDED}{padding}"));
        git(dir.path(), &["add", "src/new.rs"]);
        for args in [
            vec!["compile", "--changed", "--json"],
            vec!["compile", "src/new.rs", "--json"],
        ] {
            let out = keel(dir.path(), &args);
            assert_eq!(out.status.code(), Some(0));
            let hits = violations_with_code(&parse(&out), "W011");
            assert_eq!(hits.len(), 1, "diff.renames={default_renames}: {hits:?}");
            assert_eq!(hits[0]["line"], 3);
            assert_eq!(hits[0]["file"], "src/new.rs");
        }
        assert_eq!(
            review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
            1
        );
    }
}

#[test]
fn homes_committed_rename_since_compares_working_text_against_base() {
    let dir = fixture(true);
    git(dir.path(), &["mv", "src/lib.rs", "src/new.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "rename"]);
    let out = keel(dir.path(), &["compile", "--since", "HEAD~1", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(violations_with_code(&parse(&out), "W011").is_empty());
    write(dir.path(), "src/new.rs", ADDED);
    let out = keel(dir.path(), &["compile", "--since", "HEAD~1", "--json"]);
    assert_eq!(violations_with_code(&parse(&out), "W011").len(), 1);
}

#[test]
fn homes_unstaged_move_explicit_destination_fires_control() {
    let dir = fixture(true);
    fs::rename(dir.path().join("src/lib.rs"), dir.path().join("src/new.rs")).unwrap();
    let out = keel(dir.path(), &["compile", "--changed", "--json"]);
    assert!(violations_with_code(&parse(&out), "W011").is_empty());
    assert_eq!(
        violations_with_code(&compile_json(dir.path(), "src/new.rs"), "W011").len(),
        1
    );
}

#[test]
fn homes_rename_eligibility_compile_checks_both_paths_control() {
    for from in ["src/time.rs", "outside/lib.rs"] {
        let dir = fixture(true);
        if from.starts_with("outside") {
            write(dir.path(), from, BASE);
            commit_paths(dir.path(), &[from]);
        }
        git(dir.path(), &["mv", from, "src/new.rs"]);
        assert_eq!(
            violations_with_code(&compile_json(dir.path(), "src/new.rs"), "W011").len(),
            1
        );
    }
}

#[test]
fn homes_regex_config_write_roundtrip_and_invalid_rules_refused() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    let rules = json!([{"name": "mixed", "patterns": ["CURRENT_DATE", {"regex": "CURRENT_[A-Z]+"}], "scope": "src"}]);
    let out = keel(dir.path(), &["config", "homes", &rules.to_string()]);
    assert_eq!(out.status.code(), Some(0));
    let raw: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["homes"], rules);
    let out = keel(dir.path(), &["config", "homes"]);
    let loaded: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(loaded[0]["patterns"], rules[0]["patterns"]);
    for pattern in [
        json!({"regex": "["}),
        json!({"regex": ""}),
        json!({"regex": "a*"}),
        json!({"regex": "^$"}),
        json!({"re": "x"}),
        json!({"regex": "x", "extra": true}),
    ] {
        let before = fs::read(&path).unwrap();
        let bad = json!([{"name": "bad", "patterns": [pattern]}]);
        let out = keel(dir.path(), &["config", "homes", &bad.to_string()]);
        assert_eq!(out.status.code(), Some(1), "{pattern}");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn homes_invalid_regex_load_warns_once_per_rule_and_preserves_gate() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    cfg["review"]["gate"] = json!(["W011"]);
    for (name, pattern) in [
        ("invalid", json!({"regex": "["})),
        ("empty", json!({"regex": ""})),
        ("zero", json!({"regex": "a*"})),
        ("wrong", json!({"re": "x"})),
    ] {
        cfg["homes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": name, "patterns": [pattern]}));
    }
    fs::write(&path, cfg.to_string()).unwrap();
    write(dir.path(), "src/lib.rs", ADDED);
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    for name in ["invalid", "empty", "zero", "wrong"] {
        assert_eq!(
            stderr
                .matches(&format!("skipping home rule {name:?}:"))
                .count(),
            1
        );
    }
    assert_eq!(stderr.matches("invalid regex").count(), 3);
    assert_eq!(violations_with_code(&parse(&out), "W011").len(), 1);
    let out = review(dir.path(), "HEAD", true);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(review_hits(&out, "W011").len(), 1);
}

#[test]
fn homes_breaker_upgrade_resets_raw_comment_fingerprint_and_passive_compile() {
    let dir = fixture(true);
    config(dir.path(), "error", &[]);
    write(
        dir.path(),
        "src/q.rs",
        "let x = \"CURRENT_DATE\"; // note\n",
    );
    let store = keel_core::sqlite::SqliteGraphStore::open(
        dir.path().join(".keel/graph.db").to_str().unwrap(),
    )
    .unwrap();
    let raw = keel_core::hash::hash_string("let x = \"CURRENT_DATE\"; // note");
    store
        .save_circuit_breaker(&[(
            "E007".into(),
            "src/q.rs".into(),
            2,
            false,
            "src/q.rs".into(),
            format!("homes-v2:\n{raw}"),
        )])
        .unwrap();
    for _ in 0..2 {
        let out = keel(dir.path(), &["compile", "src/q.rs", "--json"]);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            violations_with_code(&parse(&out), "E007")[0]["severity"],
            "ERROR"
        );
        let rows = store.load_circuit_breaker().unwrap();
        let row = rows.iter().find(|r| r.0 == "E007").unwrap();
        assert_eq!(row.2, 1);
        assert!(!row.3);
        assert!(row.5.starts_with("homes-v3:\n"));
    }
}

#[test]
fn homes_literal_only_code_has_identical_baseline_and_graph_control() {
    let dir = fixture(true);
    let db = dir.path().join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    let before = store.get_nodes_in_file("src/lib.rs");
    assert!(!before.is_empty());
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(violations_with_code(&parse(&out), "W011").is_empty());
    let after = store.get_nodes_in_file("src/lib.rs");
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert!(review_hits(&review(dir.path(), "HEAD", false), "W011").is_empty());
}
