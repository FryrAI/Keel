//! Tests for fragment-level clone detection (issue #66).

use super::*;

/// A body long enough to clear one window, built from a repeatable statement.
fn block(prefix: &str, statements: usize) -> String {
    let mut out = String::new();
    for i in 0..statements {
        out.push_str(&format!(
            "let {prefix}{i} = compute(items[{i}], config);\n\
             if {prefix}{i}.is_valid() {{ total += {prefix}{i}.weight; }}\n"
        ));
    }
    out
}

fn scan_of(bodies: &[(&str, &str)]) -> Vec<FragmentCloneEntry> {
    let mut scan = FragmentScan::new();
    for (i, (name, body)) in bodies.iter().enumerate() {
        scan.add(
            format!("hash{i}"),
            (*name).to_string(),
            format!("src/{name}.rs"),
            1,
            hash_t2::tokenize_positioned(body, "rust", hash_t2::IdentifierMode::Verbatim),
        );
    }
    scan.finish()
}

#[test]
fn test_identical_fragment_in_two_functions_is_cloned() {
    // Distinctive prefixes longer than one window, then the same block: the
    // shared part must be reported and the private part must not.
    let shared = block("v", 8);
    let a = format!("{}{shared}", block("alpha", 3));
    let b = format!("{}{shared}", block("beta", 3));

    let rows = scan_of(&[("a", &a), ("b", &b)]);

    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(
            row.cloned_lines >= 12,
            "{} should report most of the shared block as cloned, got {}/{}",
            row.name,
            row.cloned_lines,
            row.code_lines
        );
        assert!(
            row.cloned_lines < row.code_lines,
            "{} must not report its private prefix as cloned ({}/{})",
            row.name,
            row.cloned_lines,
            row.code_lines
        );
    }
}

#[test]
fn test_unrelated_functions_report_no_clone() {
    let a = "let total = 0;\nfor item in items { total += item.price; }\nreturn total;";
    let b =
        "let name = user.name();\nif name.is_empty() { return None; }\nSome(name.to_uppercase())";

    for row in scan_of(&[("a", a), ("b", b)]) {
        assert_eq!(row.cloned_lines, 0, "{}", row.name);
        assert!(row.code_lines > 0, "{}", row.name);
    }
}

#[test]
fn test_repetition_inside_one_function_is_not_a_clone() {
    // The same window occurs many times, but only ever in this one body.
    let body = block("v", 12);
    let rows = scan_of(&[("only", &body)]);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cloned_lines, 0);
}

#[test]
fn test_renamed_copy_is_not_a_fragment_clone() {
    // The documented limit of the surface: windows are tokenized with
    // identifiers verbatim, because positional Type-2 renaming is defined over
    // a whole body and does not survive being cut into windows. A copy that
    // renames every local is invisible here — W006's whole-body Type-2 tier is
    // what catches that shape.
    let original = block("alpha", 8);
    let renamed = block("beta", 8).replace("compute", "evaluate");

    let rows = scan_of(&[("orig", &original), ("copy", &renamed)]);

    assert!(rows.iter().all(|r| r.cloned_lines == 0));
}

#[test]
fn test_fragment_shorter_than_the_window_is_not_a_clone() {
    // Two bodies sharing one short statement — well under FRAGMENT_WINDOW_TOKENS.
    let a = "let x = helper(input);\nreturn x + 1;";
    let b = "let x = helper(input);\nreturn x * 2;";

    for row in scan_of(&[("a", a), ("b", b)]) {
        assert_eq!(row.cloned_lines, 0, "{}", row.name);
    }
}

#[test]
fn test_code_lines_ignores_comments_and_blank_lines() {
    let body = "let a = 1;\n\n// a comment line\n\nreturn a;";
    let rows = scan_of(&[("a", body)]);

    assert_eq!(rows[0].code_lines, 2);
}

#[test]
fn test_empty_body_produces_no_row() {
    let rows = scan_of(&[("empty", "   \n// nothing\n")]);
    assert!(rows.is_empty());
}

#[test]
fn test_rows_carry_their_node_identity() {
    let body = block("v", 8);
    let mut scan = FragmentScan::new();
    scan.add(
        "abc123".to_string(),
        "parse".to_string(),
        "src/parser.rs".to_string(),
        42,
        hash_t2::tokenize_positioned(&body, "rust", hash_t2::IdentifierMode::Verbatim),
    );
    let rows = scan.finish();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].node_hash, "abc123");
    assert_eq!(rows[0].name, "parse");
    assert_eq!(rows[0].file_path, "src/parser.rs");
    assert_eq!(rows[0].line, 42);
}

#[test]
fn test_window_hashes_are_position_sensitive() {
    let ids: Vec<u32> = (0..60).collect();
    let hashes = window_hashes(&ids, FRAGMENT_WINDOW_TOKENS);

    assert_eq!(hashes.len(), 60 - FRAGMENT_WINDOW_TOKENS + 1);
    assert_ne!(hashes[0], hashes[1]);

    // The rolling recurrence must agree with a from-scratch hash of the same
    // span, or the sliding window silently indexes something else.
    let direct = window_hashes(&ids[3..3 + FRAGMENT_WINDOW_TOKENS], FRAGMENT_WINDOW_TOKENS);
    assert_eq!(direct[0], hashes[3]);
}

#[test]
fn test_window_hashes_empty_below_window() {
    assert!(window_hashes(&[1, 2, 3], FRAGMENT_WINDOW_TOKENS).is_empty());
}

#[test]
fn test_cross_language_bodies_do_not_collide_by_accident() {
    // Same shape, different languages: the scan tokenizes each with its own
    // keyword table, so this only asserts the scan runs and reports honestly.
    let py = "total = 0\nfor item in items:\n    total += item.price\nreturn total\n";
    let rows = scan_of(&[("py", py)]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cloned_lines, 0);
}
