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
