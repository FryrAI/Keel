//! Round-one regressions for Git path quoting and JavaScript JSX masking.

use super::*;

fn assert_staged_source_checked(path: &str) {
    let dir = fixture(true);
    write(dir.path(), path, BASE);
    git(dir.path(), &["add", path]);
    let out = keel(dir.path(), &["compile", "--changed", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let hits = violations_with_code(&parse(&out), "W011");
    assert_eq!(hits.len(), 1, "{path:?}: {out:?}");
    assert_eq!(hits[0]["file"], path);
    let out = review(dir.path(), "HEAD", false);
    assert_eq!(out.status.code(), Some(0));
    let result = parse(&out);
    assert_eq!(result["files_changed"], 1);
    assert_eq!(result["files_analyzed"], 1, "{path:?}: {result}");
    assert_eq!(result["unanalyzed"], json!([]));
    let hits = review_hits(&out, "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], path);
}

#[test]
fn homes_non_ascii_staged_source_compile_changed_and_review_are_checked() {
    assert_staged_source_checked("src/ä.rs");
}

#[test]
fn homes_space_staged_source_compile_changed_and_review_are_checked_control() {
    assert_staged_source_checked("src/with space.rs");
}

#[cfg(unix)]
#[test]
fn homes_tab_newline_quote_staged_sources_are_checked() {
    for path in [
        "src/with\ttab.rs",
        "src/with\nnewline.rs",
        "src/with\"quote.rs",
    ] {
        assert_staged_source_checked(path);
    }
}

#[test]
fn homes_non_ascii_rename_is_silent_and_added_occurrence_is_checked() {
    let dir = fixture(true);
    let path = "src/größe.rs";
    git(dir.path(), &["mv", "src/lib.rs", path]);
    for args in [
        vec!["compile", "--changed", "--json"],
        vec!["compile", path, "--json"],
    ] {
        let out = keel(dir.path(), &args);
        assert_eq!(out.status.code(), Some(0));
        assert!(violations_with_code(&parse(&out), "W011").is_empty());
    }
    let out = review(dir.path(), "HEAD", false);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(parse(&out)["files_analyzed"], 1);
    assert!(review_hits(&out, "W011").is_empty());

    // Keep enough unchanged text for Git to recognize the edited rename.
    write(
        dir.path(),
        path,
        &format!("{BASE}const SQL: &str = \"CURRENT_DATE\";\n"),
    );
    git(dir.path(), &["add", path]);
    let out = keel(dir.path(), &["compile", "--changed", "--json"]);
    let hits = violations_with_code(&parse(&out), "W011");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["file"], path);
    assert_eq!(hits[0]["line"], 4);
    assert_eq!(
        review_hits(&review(dir.path(), "HEAD", false), "W011").len(),
        1
    );
}

#[test]
fn homes_javascript_jsx_text_compile_and_review_preserve_occurrences() {
    for ext in ["js", "mjs", "cjs", "jsx"] {
        let dir = fixture(true);
        let path = format!("src/q.{ext}");
        write(
            dir.path(),
            &path,
            "// CURRENT_DATE\nconst view = <p>// CURRENT_DATE</p>;\n",
        );
        let hits = violations_with_code(&compile_json(dir.path(), &path), "W011");
        assert_eq!(hits.len(), 1, "{ext}: {hits:?}");
        assert_eq!(hits[0]["line"], 2);
        git(dir.path(), &["add", &path]);
        let out = review(dir.path(), "HEAD", false);
        assert_eq!(out.status.code(), Some(0));
        let hits = review_hits(&out, "W011");
        assert_eq!(hits.len(), 1, "{ext}: {hits:?}");
        assert_eq!(hits[0]["line"], 2);
    }
}

#[test]
fn homes_multiline_comment_removal_rechecks_changed_line_split_control() {
    let dir = fixture(true);
    write(
        dir.path(),
        "src/q.rs",
        "let x = \"CURRENT_DATE\"/*\n note */;\n",
    );
    git(dir.path(), &["add", "src/q.rs"]);
    git(dir.path(), &["commit", "-q", "-m", "multiline comment"]);
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
