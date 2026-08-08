//! Tests for the Type-2 (identifier/literal-normalized) body fingerprint.

use super::*;
use crate::hash::compute_body_hash;

const RUST_BODY: &str = "let total = 0;\nfor item in items { total += item.price; }\nreturn total;";

#[test]
fn test_t2_hash_deterministic() {
    assert_eq!(
        compute_t2_hash(RUST_BODY, "rust"),
        compute_t2_hash(RUST_BODY, "rust")
    );
    assert_eq!(compute_t2_hash(RUST_BODY, "rust").len(), 11);
}

#[test]
fn test_t2_hash_sees_through_renamed_identifiers() {
    let original = "let total = 0; for item in items { total += item.price; } return total;";
    let renamed = "let sum = 0; for entry in rows { sum += entry.cost; } return sum;";

    assert_ne!(
        compute_body_hash(original),
        compute_body_hash(renamed),
        "Type-1 must NOT catch this pair — otherwise the test proves nothing"
    );
    assert_eq!(
        normalize_body_t2(original, "rust"),
        normalize_body_t2(renamed, "rust")
    );
    assert_eq!(
        compute_t2_hash(original, "rust"),
        compute_t2_hash(renamed, "rust")
    );
}

#[test]
fn test_t2_hash_sees_through_literal_changes() {
    assert_eq!(
        compute_t2_hash("return 1;", "rust"),
        compute_t2_hash("return 2;", "rust")
    );
    assert_eq!(
        compute_t2_hash("log(\"started\");", "typescript"),
        compute_t2_hash("log(\"finished now\");", "typescript")
    );
}

#[test]
fn test_t2_hash_differs_on_structural_change() {
    let branching = "if (a) { return b; } else { return c; }";
    let flat = "return b;";
    assert_ne!(
        compute_t2_hash(branching, "typescript"),
        compute_t2_hash(flat, "typescript")
    );

    // Same identifiers, different operator: still a different shape.
    assert_ne!(
        compute_t2_hash("return a + b;", "typescript"),
        compute_t2_hash("return a - b;", "typescript")
    );
}

#[test]
fn test_t2_keywords_not_renamed() {
    let rust = normalize_body_t2("if self.ready { return Some(value); }", "rust");
    for kw in ["if", "self", "return", "Some"] {
        assert!(
            rust.split(' ').any(|t| t == kw),
            "`{kw}` must survive verbatim in `{rust}`"
        );
    }

    let py = normalize_body_t2("for row in rows:\n    yield self.parse(row)", "python");
    for kw in ["for", "in", "yield", "self"] {
        assert!(py.split(' ').any(|t| t == kw), "`{kw}` missing from `{py}`");
    }

    let ts = normalize_body_t2("if (this.x === undefined) { return null; }", "typescript");
    for kw in ["this", "undefined", "null", "return"] {
        assert!(ts.split(' ').any(|t| t == kw), "`{kw}` missing from `{ts}`");
    }

    let go = normalize_body_t2("if err != nil { return nil }", "go");
    assert!(go.split(' ').filter(|t| *t == "nil").count() == 2, "{go}");
}

#[test]
fn test_t2_hash_stable_across_reformatting() {
    let cases = [
        (
            "rust",
            "let x = 1;\nreturn x + 2;",
            "let  x=1;\n\n  return x  +  2;",
        ),
        (
            "python",
            "total = 0\nfor x in xs:\n    total += x\nreturn total",
            "total = 0\n\nfor x in xs:\n        total  +=  x\nreturn  total",
        ),
        (
            "typescript",
            "const a = f(b);\nreturn a;",
            "const a = f( b );\n\n    return a;",
        ),
        (
            "go",
            "sum := 0\nfor _, v := range xs {\n\tsum += v\n}\nreturn sum",
            "sum := 0\nfor _, v := range xs {\n    sum  +=  v\n}\n\nreturn sum",
        ),
    ];
    for (lang, a, b) in cases {
        assert_eq!(
            compute_t2_hash(a, lang),
            compute_t2_hash(b, lang),
            "reformatting moved the {lang} T2 hash"
        );
    }
}

#[test]
fn test_t2_comments_are_dropped() {
    assert_eq!(
        compute_t2_hash("// explain\nreturn a + b;", "rust"),
        compute_t2_hash("return a + b;", "rust")
    );
    assert_eq!(
        compute_t2_hash("# explain\nreturn a + b", "python"),
        compute_t2_hash("return a + b", "python")
    );
}

#[test]
fn test_t2_boolean_and_type_literals_collapse() {
    assert_eq!(
        compute_t2_hash("return true;", "rust"),
        compute_t2_hash("return false;", "rust")
    );
    assert_eq!(
        compute_t2_hash("return True", "python"),
        compute_t2_hash("return False", "python")
    );
    // Real types are NOT collapsed — a documented scope limit of the lexical
    // layer that keeps `i32` and `String` bodies distinct.
    assert_ne!(
        compute_t2_hash("let x: i32 = 0;", "rust"),
        compute_t2_hash("let x: f64 = 0;", "rust")
    );
}

#[test]
fn test_t2_rust_lifetimes_do_not_swallow_the_body() {
    // A lifetime tick must stay punctuation: reading it as a string opener ate
    // everything after it (the Type-1 bug this heuristic exists for).
    let body = "let s: &'a str = name; return s.len();";
    let tokens = tokenize(body, "rust");
    assert!(
        tokens.iter().any(|t| t == "return"),
        "tail of the body was swallowed: {tokens:?}"
    );
    assert_eq!(tokens[tokens.len() - 3..], ["(", ")", ";"], "{tokens:?}");
    assert_eq!(tokens.iter().filter(|t| *t == "<str>").count(), 0);
}

#[test]
fn test_t2_string_contents_do_not_leak_tokens() {
    // Code-looking text inside a literal must not become tokens.
    let tokens = tokenize("return \"if (x) { return y; }\";", "typescript");
    assert_eq!(tokens, vec!["return", "<str>", ";"]);
}

#[test]
fn test_t2_renames_are_positional_and_consistent() {
    assert_eq!(
        normalize_body_t2("a = b + a;", "typescript"),
        "v0 = v1 + v0 ;",
        "same identifier must reuse its slot"
    );
}

#[test]
fn test_min_t2_normalized_len_is_sane() {
    assert!(
        normalize_body_t2("return self.value;", "rust").len() < MIN_T2_NORMALIZED_LEN,
        "a one-line delegation must fall under the floor"
    );
    let real = "let mut out = Vec::new();\n\
                for item in items.iter() {\n\
                    if item.active && item.count > 0 { out.push(item.name.clone()); }\n\
                }\n\
                out.sort();\n\
                return out;";
    assert!(
        normalize_body_t2(real, "rust").len() >= MIN_T2_NORMALIZED_LEN,
        "a real multi-statement body must clear the floor"
    );
}

#[test]
fn test_positioned_tokens_carry_body_relative_lines() {
    let tokens = tokenize_positioned("let a = 1;\nreturn a;", "rust", IdentifierMode::Verbatim);

    assert_eq!(tokens[0].text, "let");
    assert_eq!(tokens[0].line, 0);
    assert_eq!(tokens.last().unwrap().text, ";");
    assert_eq!(tokens.last().unwrap().line, 1);
}

#[test]
fn test_positioned_lines_survive_a_multiline_string() {
    // The literal collapses to one token but spans two lines: whatever follows
    // it must still be attributed to the line it is actually on.
    let tokens = tokenize_positioned(
        "let s = \"one\ntwo\";\nreturn s;",
        "rust",
        IdentifierMode::Verbatim,
    );
    let ret = tokens
        .iter()
        .find(|t| t.text == "return")
        .expect("return token");
    assert_eq!(ret.line, 2);
}

#[test]
fn test_verbatim_mode_keeps_identifiers_and_still_collapses_literals() {
    let tokens: Vec<String> =
        tokenize_positioned("let total = 42;", "rust", IdentifierMode::Verbatim)
            .into_iter()
            .map(|t| t.text.into_owned())
            .collect();

    assert_eq!(tokens, vec!["let", "total", "=", "<int>", ";"]);
}

/// `keel map` tokenizes each body ONCE, verbatim, and derives the Type-2
/// fingerprint from that stream. The derivation must be indistinguishable from
/// a direct renamed tokenization, in every language's keyword table, or stored
/// and compile-time fingerprints stop matching.
#[test]
fn test_rename_and_join_matches_a_direct_renamed_tokenization() {
    let bodies = [
        (
            "rust",
            "let mut total = 0;\nfor item in items {\n    if item.ok { total += item.n; }\n}\ntotal",
        ),
        (
            "python",
            "total = 0\nfor item in items:\n    if item.ok:\n        total += item.n\nreturn total",
        ),
        (
            "go",
            "total := 0\nfor _, item := range items {\n\tif item.ok {\n\t\ttotal += item.n\n\t}\n}\nreturn total",
        ),
        (
            "typescript",
            "let total = 0;\nfor (const item of items) {\n  if (item.ok) { total += item.n; }\n}\nreturn total;",
        ),
    ];
    for (lang, body) in bodies {
        let verbatim = tokenize_positioned(body, lang, IdentifierMode::Verbatim);
        assert_eq!(
            rename_and_join(&verbatim, lang),
            normalize_body_t2(body, lang),
            "{lang}: derived rename must match a direct one"
        );
    }
}

/// Why fragment matching cannot use the renamed stream: the same statement gets
/// different numbers depending on what preceded it in the body.
#[test]
fn test_renaming_is_relative_to_the_body_not_the_statement() {
    let with_prefix = tokenize("let unrelated = 1; let total = 2;", "rust");
    let without = tokenize("let total = 2;", "rust");

    assert_eq!(without[1], "v0");
    assert_eq!(with_prefix[6], "v1", "same statement, different rename");
}

#[test]
fn test_t2_rust_raw_string_does_not_swallow_the_tail() {
    // A raw string whose contents hold a quote and a `/*` used to shift
    // quote parity in strip_comments: the tail was stripped as one block
    // comment, and two bodies with COMPLETELY different tails fingerprinted
    // identically (the reachable W006 Type-2 false positive from the
    // adversarial probe).
    let prefix = r##"{
    let config_source = r#"members = ["crates/*"]"#;
    let parsed = parse_workspace(config_source);
"##;
    let a = format!(
        "{prefix}    for package in parsed.members {{ register(package); }}\n    Ok(a_specific_result)\n}}"
    );
    let b = format!("{prefix}    return Err(TotallyDifferentError::NotAWorkspace);\n}}");
    assert_ne!(
        compute_t2_hash(&a, "rust"),
        compute_t2_hash(&b, "rust"),
        "different tails after a raw string must fingerprint differently"
    );
}

#[test]
fn test_t2_rename_invariance_holds_for_non_ascii_identifiers() {
    // Non-ASCII identifiers must be ONE renameable token, not per-byte
    // fallback tokens (which are never renamed and broke Type-2's whole
    // purpose on unicode-identifier codebases).
    let body_a = "{ let 变量 = compute(); let café = 变量 + other(变量); finish(café, 变量, other_thing, more_stuff); }";
    let body_b = "{ let renamed = compute(); let refill = renamed + other(renamed); finish(refill, renamed, other_thing, more_stuff); }";
    assert_eq!(
        normalize_body_t2(body_a, "rust"),
        normalize_body_t2(body_b, "rust"),
        "a CJK/accented rename-pair must normalize identically to an ASCII one"
    );
    let tokens = tokenize_positioned(body_a, "rust", IdentifierMode::Verbatim);
    assert!(
        tokens.iter().any(|t| t.text == "café") && tokens.iter().any(|t| t.text == "变量"),
        "non-ASCII identifiers must survive as single verbatim tokens"
    );
}

#[test]
fn test_t2_raw_string_with_odd_embedded_quotes_keeps_the_tail() {
    // strip_comments got the raw-string fix first; the tokenizer's own
    // skip_string still paired quotes generically, so an ODD number of
    // embedded quotes swallowed everything after the raw string as one
    // <str> token (round-2 reviewer finding — the earlier regression test
    // had a coincidentally even count).
    let prefix = "{ let msg = r#\"Use \" to open a string\"#;\n";
    let a = format!("{prefix}    finish_one_way(msg, extra, more_args_here) }}");
    let b = format!("{prefix}    completely_different_tail(msg) }}");
    assert_ne!(
        compute_t2_hash(&a, "rust"),
        compute_t2_hash(&b, "rust"),
        "different tails after an odd-quote raw string must fingerprint differently"
    );
}

#[test]
fn test_t2_go_backtick_raw_string_is_one_str_token() {
    let body_a = "{ p := `C:\\`; use(p, second_arg, third_arg, fourth_arg) }";
    let body_b = "{ p := `D:\\`; use(p, second_arg, third_arg, fourth_arg) }";
    assert_eq!(
        normalize_body_t2(body_a, "go"),
        normalize_body_t2(body_b, "go"),
        "backtick contents collapse to <str> — literal values never split a T2 match"
    );
    // A STRUCTURAL tail change (renamed identifiers rightly collapse).
    let tail_change = "{ p := `C:\\`; if ok { use(p, second_arg, third_arg, fourth_arg) } }";
    assert_ne!(
        normalize_body_t2(body_a, "go"),
        normalize_body_t2(tail_change, "go"),
        "code after the raw string stays visible"
    );
}
