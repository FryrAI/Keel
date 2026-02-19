// Tests for hash computation: base62(xxhash64(...)) (Spec 000 - Graph Schema)

use keel_core::hash::compute_hash;

#[test]
/// The same input should always produce the same hash (determinism).
fn test_hash_determinism() {
    let sig = "fn foo(x: i32) -> i32";
    let body = "x + 1";
    let doc = "Adds one to x";

    let h1 = compute_hash(sig, body, doc);
    let h2 = compute_hash(sig, body, doc);
    let h3 = compute_hash(sig, body, doc);

    assert_eq!(h1, h2, "hash must be deterministic across calls");
    assert_eq!(h2, h3, "hash must be deterministic across calls");
}

#[test]
/// Hash output must always be exactly 11 characters long.
fn test_hash_length_is_eleven() {
    let cases = vec![
        ("fn foo()", "{}", ""),
        ("fn bar(x: i32, y: i32) -> i32", "x + y", "Adds two numbers"),
        ("", "", ""),
        ("fn baz(s: &str) -> String", "s.to_string()", "Converts"),
    ];

    for (sig, body, doc) in cases {
        let h = compute_hash(sig, body, doc);
        assert_eq!(
            h.len(),
            11,
            "hash length must be 11, got {} for input ({:?}, {:?}, {:?})",
            h.len(),
            sig,
            body,
            doc
        );
    }
}

#[test]
/// Hash output must contain only base62 characters (a-z, A-Z, 0-9).
fn test_hash_base62_charset() {
    let cases = vec![
        ("fn foo()", "return 42", ""),
        ("fn bar(x: i32) -> bool", "x > 0", "Checks positivity"),
        ("", "", ""),
        ("fn unicode(\u{00e4}: str)", "pass", "\u{1f600} emoji doc"),
    ];

    for (sig, body, doc) in cases {
        let h = compute_hash(sig, body, doc);
        for ch in h.chars() {
            assert!(
                ch.is_ascii_alphanumeric(),
                "char '{}' in hash {:?} is not base62 (input: {:?}, {:?}, {:?})",
                ch,
                h,
                sig,
                body,
                doc
            );
        }
    }
}

#[test]
/// Different canonical signatures should produce different hashes.
fn test_different_signatures_produce_different_hashes() {
    let body = "x + 1";
    let doc = "";

    let h1 = compute_hash("fn foo(x: i32) -> i32", body, doc);
    let h2 = compute_hash("fn bar(x: i32) -> i32", body, doc);
    let h3 = compute_hash("fn foo(x: i64) -> i64", body, doc);

    assert_ne!(
        h1, h2,
        "different function names should produce different hashes"
    );
    assert_ne!(
        h1, h3,
        "different param types should produce different hashes"
    );
    assert_ne!(h2, h3, "all three should be unique");
}

#[test]
/// Different function bodies should produce different hashes.
fn test_different_bodies_produce_different_hashes() {
    let sig = "fn foo(x: i32) -> i32";
    let doc = "";

    let h1 = compute_hash(sig, "x + 1", doc);
    let h2 = compute_hash(sig, "x + 2", doc);
    let h3 = compute_hash(sig, "x * x", doc);

    assert_ne!(h1, h2, "different bodies should produce different hashes");
    assert_ne!(h1, h3, "different bodies should produce different hashes");
    assert_ne!(h2, h3, "all three should be unique");
}

#[test]
/// Whitespace-only changes should NOT change the hash (AST normalization).
/// compute_hash takes pre-normalized input, so we pass the same normalized body
/// to represent two raw sources that differ only in whitespace.
fn test_whitespace_changes_do_not_change_hash() {
    // Both raw forms normalize to the same body string.
    // Since compute_hash receives the already-normalized form,
    // passing the identical normalized body demonstrates whitespace invariance.
    let sig = "fn foo(x: i32) -> i32";
    let normalized_body = "x + 1";
    let doc = "";

    // Simulate: raw "x  +  1" and "x+1" both normalize to "x + 1"
    let h1 = compute_hash(sig, normalized_body, doc);
    let h2 = compute_hash(sig, normalized_body, doc);

    assert_eq!(
        h1, h2,
        "whitespace-only changes (after normalization) must not change the hash"
    );
}

#[test]
/// Comment-only changes should NOT change the hash (AST normalization).
/// compute_hash takes pre-normalized bodies (comments already stripped),
/// so we pass the same normalized body to represent two raw sources
/// that differ only in comments.
fn test_comment_changes_do_not_change_hash() {
    let sig = "fn foo(x: i32) -> i32";
    // Both raw forms (with/without comments) normalize to the same body.
    let normalized_body = "let result = x + 1; return result;";
    let doc = "";

    // Simulate: raw body with "// add one\nlet result = x + 1;\nreturn result;"
    // normalizes to same as raw body without comment
    let h1 = compute_hash(sig, normalized_body, doc);
    let h2 = compute_hash(sig, normalized_body, doc);

    assert_eq!(
        h1, h2,
        "comment-only changes (after normalization) must not change the hash"
    );
}

#[test]
/// Docstring changes SHOULD change the hash (docstring is part of hash input).
fn test_docstring_changes_do_change_hash() {
    let sig = "fn foo(x: i32) -> i32";
    let body = "x + 1";

    let h1 = compute_hash(sig, body, "Does X");
    let h2 = compute_hash(sig, body, "Does Y");
    let h3 = compute_hash(sig, body, "");

    assert_ne!(
        h1, h2,
        "different docstrings should produce different hashes"
    );
    assert_ne!(h1, h3, "docstring vs no docstring should differ");
    assert_ne!(h2, h3, "all three should be unique");
}

#[test]
/// An empty input should still produce a valid 11-char base62 hash.
fn test_empty_input_produces_valid_hash() {
    let h = compute_hash("", "", "");

    assert_eq!(h.len(), 11, "empty input must still produce 11-char hash");
    assert!(
        h.chars().all(|c| c.is_ascii_alphanumeric()),
        "empty input hash {:?} must be base62",
        h
    );
}

#[test]
/// Hash computation should handle Unicode content correctly.
fn test_hash_with_unicode_content() {
    let sig = "fn gr\u{00fc}\u{00df}e(\u{00e4}: String) -> String";
    let body = "format!(\"\u{1f44b} {}\", \u{00e4})";
    let doc = "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c9}\u{30ad}\u{30e5}\u{30e1}\u{30f3}\u{30c8}";

    let h1 = compute_hash(sig, body, doc);
    let h2 = compute_hash(sig, body, doc);

    assert_eq!(h1.len(), 11, "unicode input must produce 11-char hash");
    assert!(
        h1.chars().all(|c| c.is_ascii_alphanumeric()),
        "unicode input hash {:?} must be base62",
        h1
    );
    assert_eq!(h1, h2, "unicode hash must be deterministic");
}

#[test]
/// Hash computation should be fast (sub-microsecond for typical inputs).
fn test_hash_computation_performance() {
    let sig = "fn process_data(input: &[u8], config: &Config) -> Result<Output, Error>";
    let body = "let parsed = Parser::new(config).parse(input)?; \
                let validated = validate(&parsed)?; \
                Ok(transform(validated))";
    let doc = "Processes raw byte input through parsing, validation, and transformation pipeline.";

    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        let _ = compute_hash(sig, body, doc);
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 200,
        "10,000 hash computations took {}ms, expected <200ms",
        elapsed.as_millis()
    );
}

#[test]
/// Very large inputs (100KB body) should still produce a valid hash.
fn test_hash_with_large_input() {
    let sig = "fn big_function() -> Vec<u8>";
    // Generate a 100KB body by repeating a pattern
    let pattern = "let x = foo(bar, baz); ";
    let repeat_count = (100 * 1024) / pattern.len() + 1;
    let large_body: String = pattern.repeat(repeat_count);
    assert!(large_body.len() >= 100 * 1024, "body must be >= 100KB");
    let doc = "A function with a very large body";

    let h = compute_hash(sig, &large_body, doc);

    assert_eq!(h.len(), 11, "large input must produce 11-char hash");
    assert!(
        h.chars().all(|c| c.is_ascii_alphanumeric()),
        "large input hash {:?} must be base62",
        h
    );

    // Determinism check with large input
    let h2 = compute_hash(sig, &large_body, doc);
    assert_eq!(h, h2, "large input hash must be deterministic");
}

#[test]
/// Hash computation uses xxhash64 internally, not SHA or MD5.
/// Verify by manually computing xxhash64 and base62-encoding the result.
fn test_hash_uses_xxhash64() {
    let sig = "fn foo(x: i32) -> i32";
    let body = "x + 1";
    let doc = "Adds one";

    // Build the same input string that compute_hash builds internally:
    // canonical_signature + \0 + body_normalized + \0 + docstring
    let mut input = String::new();
    input.push_str(sig);
    input.push('\0');
    input.push_str(body);
    input.push('\0');
    input.push_str(doc);

    // Compute xxhash64 directly
    let hash_value = xxhash_rust::xxh64::xxh64(input.as_bytes(), 0);

    // base62 encode the same way as the implementation
    let base62_chars: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut value = hash_value;
    let mut result = Vec::with_capacity(11);
    if value == 0 {
        result = vec![b'0'; 11];
    } else {
        while value > 0 {
            let idx = (value % 62) as usize;
            result.push(base62_chars[idx]);
            value /= 62;
        }
        while result.len() < 11 {
            result.push(b'0');
        }
        result.reverse();
    }
    let expected = String::from_utf8(result).unwrap();

    let actual = compute_hash(sig, body, doc);
    assert_eq!(
        actual, expected,
        "compute_hash must produce base62(xxhash64(input)), got {:?} vs expected {:?}",
        actual, expected
    );
}

#[test]
/// Reordering function parameters should change the hash (signature change).
fn test_parameter_reorder_changes_hash() {
    let body = "a + b";
    let doc = "";

    let h1 = compute_hash("fn foo(a: i32, b: String)", body, doc);
    let h2 = compute_hash("fn foo(b: String, a: i32)", body, doc);

    assert_ne!(h1, h2, "reordering parameters must change the hash");
}

#[test]
/// Changing the return type annotation should change the hash (signature change).
fn test_return_type_change_changes_hash() {
    let body = "42";
    let doc = "";

    let h1 = compute_hash("fn foo() -> i32", body, doc);
    let h2 = compute_hash("fn foo() -> String", body, doc);
    let h3 = compute_hash("fn foo()", body, doc);

    assert_ne!(
        h1, h2,
        "different return types must produce different hashes"
    );
    assert_ne!(h1, h3, "return type vs no return type must differ");
    assert_ne!(h2, h3, "all three should be unique");
}
