//! Review counter-cases for configuration, move pooling, and breaker attempts.

use super::*;

#[test]
fn homes_subdirectory_git_scoped_compile_preserves_base_graph_paths() {
    use keel_core::store::GraphStore;
    let dir = fixture(true);
    write(dir.path(), "src/lib.rs", ADDED);
    let out = keel(&dir.path().join("src"), &["compile", "--changed", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(violations_with_code(&parse(&out), "W011").len(), 1);
    git(dir.path(), &["add", "src/lib.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "modified query"]);
    let out = keel(
        &dir.path().join("src"),
        &["compile", "--since", "HEAD~1", "--json"],
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(violations_with_code(&parse(&out), "W011").len(), 1);
    keel(&dir.path().join("src"), &["compile", "--json"]);
    let db = dir.path().join(".keel/graph.db");
    let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
    assert!(store.get_nodes_in_file("lib.rs").is_empty());
    assert!(!store.get_nodes_in_file("src/lib.rs").is_empty());
}

#[test]
fn homes_review_rejects_a_range_base_with_the_explicit_resolution_error() {
    let dir = fixture(true);
    let out = review(dir.path(), "HEAD...HEAD", false);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cannot resolve base ref \"HEAD...HEAD\"")
    );
}

#[test]
fn homes_compile_breaker_counts_file_attempts_and_clears_after_resolution() {
    for symbol_less in [false, true] {
        let dir = fixture(true);
        config(dir.path(), "error", &[]);
        let source = |value| {
            if symbol_less {
                format!("const SQL: &str = \"CURRENT_DATE {value}\";\nconst OTHER: &str = \"CURRENT_DATE second\";\n")
            } else {
                format!("fn a() {{ let _ = \"CURRENT_DATE\"; }}\nfn b() {{ let _ = \"CURRENT_DATE\"; }}\nfn query() {{\n    let _ = \"CURRENT_DATE {value}\";\n}}\n")
            }
        };
        for attempt in 1..=3 {
            write(dir.path(), "src/lib.rs", &source(attempt));
            for _ in 0..5 {
                let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
                let hits = violations_with_code(&parse(&out), "E007");
                assert_eq!(hits.len(), if symbol_less { 2 } else { 3 });
                assert!(hits
                    .iter()
                    .all(|v| v["severity"] == if attempt < 3 { "ERROR" } else { "WARNING" }));
                assert_eq!(out.status.code(), Some(if attempt < 3 { 1 } else { 0 }));
                assert!(hits
                    .iter()
                    .all(|v| !v["fix_hint"].as_str().unwrap().contains("keel discover")));
            }
        }
        write(dir.path(), "src/lib.rs", "fn query() {}\n");
        assert!(violations_with_code(&compile_json(dir.path(), "src/lib.rs"), "E007").is_empty());
        write(dir.path(), "src/lib.rs", &source(4));
        assert_eq!(
            keel(dir.path(), &["compile", "src/lib.rs", "--json"])
                .status
                .code(),
            Some(1)
        );
    }
}

#[test]
fn homes_config_set_preserves_raw_rules_and_unknown_keys_and_refuses_invalid_json() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    let mut expected: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    expected["homes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "broken", "patterns": 42}));
    expected["unknown"] = json!({"keep": [true, 42]});
    fs::write(&path, expected.to_string()).unwrap();
    // Placement is absent on disk: adding this defaulted setting patches only that key.
    let out = keel(dir.path(), &["config", "enforce.placement", "false"]);
    assert_eq!(out.status.code(), Some(0));
    expected["enforce"]["placement"] = json!(false);
    let actual: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(actual, expected);
    let out = keel(dir.path(), &["init", "--update-docs"]);
    assert_eq!(out.status.code(), Some(0));
    let actual: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(actual, expected);
    for text in ["{bad json", "[]", "null"] {
        fs::write(&path, text).unwrap();
        let out = Command::new(keel_bin())
            .args(["config", "enforce.placement", "true"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("refusing to rewrite")
                || String::from_utf8_lossy(&out.stderr).contains("invalid object")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        let out = Command::new(keel_bin())
            .args(["init", "--update-docs"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn homes_config_typo_warns_once_without_losing_the_gate() {
    let dir = fixture(true);
    config(dir.path(), "Error", &["W011"]);
    write(dir.path(), "src/lib.rs", ADDED);
    let out = review(dir.path(), "HEAD", true);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(review_hits(&out, "W011").len(), 1);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr
            .matches("unknown enforce.homes value \"Error\"")
            .count(),
        1
    );
    assert!(!stderr.contains("using defaults"));
}

#[test]
fn homes_review_gate_warns_for_the_unavailable_home_code_only_when_gating() {
    for (severity, wrong_code, emitted) in [("error", "W011", "E007"), ("warning", "E007", "W011")]
    {
        let dir = fixture(true);
        config(dir.path(), severity, &[wrong_code]);
        write(dir.path(), "src/lib.rs", ADDED);
        let out = review(dir.path(), "HEAD", true);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(review_hits(&out, emitted).len(), 1);
        let expected =
            format!("review.gate names {wrong_code}, but enforce.homes can only emit {emitted}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr)
                .matches(&expected)
                .count(),
            1
        );
        assert!(
            !String::from_utf8_lossy(&review(dir.path(), "HEAD", false).stderr).contains(&expected)
        );
    }
}

#[test]
fn homes_review_and_compile_changed_cancel_split_module_moves_but_report_new_copy() {
    let dir = fixture(true);
    config(dir.path(), "error", &["E007"]);
    write(dir.path(), "src/lib.rs", "fn query() {}\n");
    write(dir.path(), "src/query.rs", BASE);
    git(dir.path(), &["add", "src/query.rs"]);
    let out = review(dir.path(), "HEAD", true);
    assert_eq!(out.status.code(), Some(0));
    assert!(review_hits(&out, "E007").is_empty());
    let out = keel(dir.path(), &["compile", "--changed", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(violations_with_code(&parse(&out), "E007").is_empty());
    // The destination alone has no checked removal to cancel its new occurrence.
    assert_eq!(
        violations_with_code(&compile_json(dir.path(), "src/query.rs"), "E007").len(),
        1
    );
    write(dir.path(), "src/query.rs", ADDED);
    let out = review(dir.path(), "HEAD", true);
    let hits = review_hits(&out, "E007");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["line"], 3);
    assert_eq!(hits[0]["file"], "src/query.rs");
}

#[test]
fn homes_pool_counts_deleted_files_and_does_not_credit_home_or_out_of_scope() {
    for from in ["src/deep/old.rs", "src/time.rs", "outside/old.rs"] {
        let dir = fixture(true);
        write(
            dir.path(),
            from,
            &format!("{}{BASE}", "// unrelated source padding\n".repeat(20)),
        );
        git(dir.path(), &["add", from]);
        git(dir.path(), &["commit", "-q", "-m", "move source"]);
        fs::remove_file(dir.path().join(from)).unwrap();
        if from.contains("deep") {
            fs::remove_dir(dir.path().join("src/deep")).unwrap();
        }
        write(dir.path(), "src/new.rs", BASE);
        git(dir.path(), &["add", from, "src/new.rs"]);
        for out in [
            review(dir.path(), "HEAD", false),
            keel(dir.path(), &["compile", "--changed", "--json"]),
        ] {
            let hits = if parse(&out).get("new_violations").is_some() {
                review_hits(&out, "W011")
            } else {
                violations_with_code(&parse(&out), "W011")
            };
            assert_eq!(
                hits.len(),
                usize::from(from != "src/deep/old.rs"),
                "{from}: {hits:?}"
            );
        }
    }
}

#[test]
fn homes_glob_spellings_apply_scope_and_home_eligibility() {
    let dir = fixture(true);
    for scope in [
        "src/", "./src", "././src/", ".", "./", "/", "/src", ".//src",
    ] {
        for home in ["src/time/", "./src/time", "././src/time/", "/src/time"] {
            let path = dir.path().join(".keel/keel.json");
            let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            cfg["homes"][0]["scope"] = json!(scope);
            cfg["homes"][0]["home"] = json!(home);
            fs::write(&path, cfg.to_string()).unwrap();
            write(dir.path(), "src/time/new.rs", ADDED);
            write(dir.path(), "src/new.rs", BASE);
            assert!(
                violations_with_code(&compile_json(dir.path(), "src/time/new.rs"), "W011")
                    .is_empty()
            );
            assert_eq!(
                violations_with_code(&compile_json(dir.path(), "src/new.rs"), "W011").len(),
                1
            );
        }
    }
}

#[test]
fn homes_duplicate_normalized_targets_do_not_inflate_the_move_pool() {
    let dir = fixture(true);
    write(dir.path(), "src/lib.rs", "fn query() {}\n");
    write(dir.path(), "src/new.rs", &format!("{BASE}{BASE}"));
    let ordinary = keel(
        dir.path(),
        &["compile", "src/lib.rs", "src/new.rs", "--json"],
    );
    let aliased = keel(
        dir.path(),
        &[
            "compile",
            "src/lib.rs",
            "src/../src/lib.rs",
            "src/new.rs",
            "--json",
        ],
    );
    let expected = violations_with_code(&parse(&ordinary), "W011");
    assert_eq!(expected.len(), 1);
    assert_eq!(violations_with_code(&parse(&aliased), "W011"), expected);
}

#[test]
fn homes_breaker_fixing_one_line_at_a_time_resets_attempts_and_keeps_errors() {
    let dir = fixture(true);
    config(dir.path(), "error", &[]);
    // First reach attempt two with a real edit, then shrink the identities.
    for (version, remaining) in [(1, 4), (2, 4), (2, 3), (2, 2), (2, 1)] {
        let source = (0..remaining)
            .map(|i| {
                format!(
                    "const SQL_{i}: &str = \"CURRENT_DATE {}\";\n",
                    if i == 0 { version * 10 } else { i }
                )
            })
            .collect::<String>();
        write(dir.path(), "src/lib.rs", &source);
        let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
        assert_eq!(out.status.code(), Some(1));
        let hits = violations_with_code(&parse(&out), "E007");
        assert_eq!(hits.len(), remaining);
        assert!(hits.iter().all(|v| v["severity"] == "ERROR"));
        let db = dir.path().join(".keel/graph.db");
        let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
        let rows = store.load_circuit_breaker().unwrap();
        let state = rows.iter().find(|row| row.0 == "E007").unwrap();
        assert_eq!(state.2, if version == 2 && remaining == 4 { 2 } else { 1 });
        assert!(!state.3);
        assert!(state.5.starts_with("homes-v3:\n"));
    }
}

#[test]
fn homes_breaker_unchanged_alternating_file_sets_keep_errors() {
    let dir = fixture(true);
    config(dir.path(), "error", &[]);
    write(dir.path(), "src/lib.rs", "fn query() {}\n");
    write(dir.path(), "src/q.rs", ADDED);
    let mut previous = None;
    for run in 0..5 {
        let mut args = vec!["compile", "src/q.rs"];
        if run % 2 == 1 {
            args.push("src/lib.rs");
        }
        args.push("--json");
        let out = keel(dir.path(), &args);
        assert_eq!(out.status.code(), Some(1));
        let hits = violations_with_code(&parse(&out), "E007");
        assert_eq!(hits.len(), if run % 2 == 0 { 2 } else { 1 });
        assert!(hits.iter().all(|v| v["severity"] == "ERROR"));
        let db = dir.path().join(".keel/graph.db");
        let store = keel_core::sqlite::SqliteGraphStore::open(db.to_str().unwrap()).unwrap();
        let rows = store.load_circuit_breaker().unwrap();
        let state = rows.iter().find(|row| row.0 == "E007").unwrap();
        assert_eq!(state.2, 1);
        if let Some(ref fingerprint) = previous {
            assert_eq!(&state.5, fingerprint);
        }
        previous = Some(state.5.clone());
    }
}

#[test]
fn homes_config_set_rejects_invalid_lenient_values_without_writing() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    let before = fs::read_to_string(&path).unwrap();
    for (key, value, diagnostic) in [
        ("enforce.homes", "Error", "must be warning or error"),
        ("enforce.homes", "null", "must be warning or error"),
        ("homes", "x", "must be a JSON array"),
        ("homes", "{}", "must be a JSON array"),
    ] {
        let out = keel(dir.path(), &["config", key, value]);
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains(diagnostic));
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }
    for severity in ["warning", "error"] {
        assert_eq!(
            keel(dir.path(), &["config", "enforce.homes", severity])
                .status
                .code(),
            Some(0)
        );
    }
    assert_eq!(
        keel(dir.path(), &["config", "homes", "[]"]).status.code(),
        Some(0)
    );
    let cfg: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(cfg["homes"], json!([]));
}

#[test]
fn homes_init_merge_refuses_invalid_config_without_writing() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    for text in ["{bad json\n", "[]", "null"] {
        fs::write(&path, text).unwrap();
        let out = Command::new(keel_bin())
            .args(["init", "--merge", "--yes"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("refusing to rewrite config"));
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn homes_config_rewrites_preserve_trailing_newline() {
    let dir = fixture(true);
    let path = dir.path().join(".keel/keel.json");
    let original = fs::read_to_string(&path).unwrap();
    for newline in ["", "\n"] {
        for args in [
            vec!["config", "enforce.homes", "error"],
            vec!["init", "--update-docs"],
            vec!["init", "--merge", "--yes"],
        ] {
            fs::write(&path, format!("{original}{newline}")).unwrap();
            let out = keel(dir.path(), &args);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                fs::read_to_string(&path).unwrap().ends_with('\n'),
                !newline.is_empty()
            );
        }
    }
}

#[test]
fn homes_parent_globs_warn_and_skip_the_rule_without_losing_the_gate() {
    let dir = fixture(true);
    config(dir.path(), "warning", &["W011"]);
    let path = dir.path().join(".keel/keel.json");
    let original: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    write(dir.path(), "src/lib.rs", ADDED);
    for key in ["home", "scope"] {
        let mut cfg = original.clone();
        let mut bad = json!({"name": "parent", "patterns": ["CURRENT_DATE"]});
        bad[key] = json!("src/../time");
        cfg["homes"].as_array_mut().unwrap().push(bad);
        fs::write(&path, cfg.to_string()).unwrap();
        let out = review(dir.path(), "HEAD", true);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(review_hits(&out, "W011").len(), 1);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(stderr.matches("skipping home rule \"parent\"").count(), 1);
        assert!(stderr.contains("parent component '..'"));
    }
}
