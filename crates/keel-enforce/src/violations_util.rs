/// Compute the (plain, disambiguated) hash pair for a definition — the two
/// identities `keel map` may have stored for it (map salts with the file
/// path on collision). Single source of truth for "does this def match its
/// stored node", shared by the engine's hash sync and progressive adoption.
pub fn definition_hashes(
    def: &keel_parsers::resolver::Definition,
    file_path: &str,
) -> (String, String) {
    // Normalize the body once and reuse it for both identities — routing
    // through `Definition::{hash, hash_disambiguated}` would normalize twice.
    let doc = def.docstring.as_deref().unwrap_or("");
    let body = def.body_for_hash();
    (
        keel_core::hash::compute_hash(&def.signature, &body, doc),
        keel_core::hash::compute_hash_disambiguated(&def.signature, &body, doc, file_path),
    )
}

/// Whether a stored node's hash matches the definition under either of the
/// two identities from [`definition_hashes`].
pub fn node_hash_matches(
    node: &keel_core::types::GraphNode,
    def: &keel_parsers::resolver::Definition,
    file_path: &str,
) -> bool {
    let (hash, hash_d) = definition_hashes(def, file_path);
    node.hash == hash || node.hash == hash_d
}

/// Check if a file is a benchmark file — harness-invoked, like tests, so
/// dead-code analysis doesn't apply.
pub fn is_bench_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with("benches/") || normalized.contains("/benches/")
}

/// Check if a file is a declaration/stub file: signature-only by definition,
/// so docstring and body-level checks don't apply (`.pyi` stubs, `.d.ts`
/// declarations).
pub fn is_stub_file(path: &str) -> bool {
    path.ends_with(".pyi")
        || path.ends_with(".d.ts")
        || path.ends_with(".d.mts")
        || path.ends_with(".d.cts")
}

/// Check if a file path is a test file by language convention.
/// Patterns: *_test.go, test_*.py, *_test.py, *.test.ts, *.spec.ts,
/// *.test.js, *.spec.js, *_test.rs, *_tests.rs, tests.rs
pub fn is_test_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");

    // Directory-based: files inside tests/, __tests__/, test/ directories
    // Check both mid-path ("/tests/") and top-level ("tests/") variants
    if normalized.contains("/tests/")
        || normalized.contains("/__tests__/")
        || normalized.contains("/test/")
        || normalized.starts_with("tests/")
        || normalized.starts_with("__tests__/")
        || normalized.starts_with("test/")
    {
        return true;
    }

    let basename = normalized.rsplit('/').next().unwrap_or(&normalized);

    // Go: *_test.go
    if basename.ends_with("_test.go") {
        return true;
    }
    // Python: test_*.py or *_test.py
    if basename.ends_with(".py")
        && (basename.starts_with("test_") || basename.ends_with("_test.py"))
    {
        return true;
    }
    // TypeScript/JavaScript: *.test.ts, *.spec.ts, *.test.js, *.spec.js, *.test.tsx, *.spec.tsx
    if basename.contains(".test.") || basename.contains(".spec.") {
        return true;
    }
    // Rust: *_test.rs, *_tests.rs (plural — this repo's own `#[path =
    // "..._tests.rs"] mod tests;` convention), or exactly tests.rs.
    // Suffix match, not substring: "utils_tests.rs" matches, "contests.rs"
    // does not (no underscore before "tests.rs").
    if basename.ends_with("_test.rs") || basename.ends_with("_tests.rs") || basename == "tests.rs" {
        return true;
    }
    false
}

/// Count parameters from a signature string. Returns 0 if unable to parse.
pub fn count_params(sig: &str) -> usize {
    let Some(start) = sig.find('(') else { return 0 };
    let Some(end) = sig.find(')') else { return 0 };
    let params = &sig[start + 1..end].trim();
    if params.is_empty() {
        return 0;
    }
    params.split(',').count()
}

/// Count args in a call expression. Rough heuristic — returns 0 if cannot parse.
pub fn count_call_args(name: &str) -> usize {
    // In practice, the parser provides arg count. This is a fallback.
    let Some(start) = name.find('(') else {
        return 0;
    };
    let Some(end) = name.rfind(')') else { return 0 };
    let args = &name[start + 1..end].trim();
    if args.is_empty() {
        return 0;
    }
    args.split(',').count()
}

/// Strip all whitespace so signatures can be compared ignoring pure
/// reformatting. A simple "collapse runs of whitespace to one space" would
/// still miscompare the most common rustfmt/prettier wrap style — one
/// parameter per line, with the `(`/`)` glued to a newline that has no
/// corresponding space in the single-line form:
/// ```text
/// fn foo(x: i32, y: i32) -> bool
/// fn foo(
///     x: i32,
///     y: i32,
/// ) -> bool
/// ```
/// Collapsing would leave `foo( x: i32...` vs `foo(x: i32...`, still
/// unequal. Removing whitespace entirely instead of collapsing it handles
/// this correctly — the token stream is what actually defines the
/// signature, and whitespace never distinguishes two different signatures.
pub fn normalize_signature(sig: &str) -> String {
    sig.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Split `name` into (start, end) byte ranges for each "word" segment, using
/// snake_case underscores if present, else camelCase upper-case transitions.
fn segment_ranges(name: &str) -> Vec<(usize, usize)> {
    if name.contains('_') {
        let mut ranges = Vec::new();
        let mut start = 0;
        for (i, c) in name.char_indices() {
            if c == '_' {
                if i > start {
                    ranges.push((start, i));
                }
                start = i + c.len_utf8();
            }
        }
        if start < name.len() {
            ranges.push((start, name.len()));
        }
        return ranges;
    }

    // camelCase, with acronym runs treated as one segment: a new segment
    // starts at an uppercase char only when the previous char is lowercase
    // (a plain lower->upper transition, e.g. "parse"|"Header"), or when the
    // previous char is ALSO uppercase but the next char is lowercase (this
    // is the last capital of a run, which kicks off the next word, e.g. the
    // second "H" in "HTTPHeader" starts "Header" while "HTTP" stays whole).
    // Standard camel tokenization: "parseHTTPHeader" -> parse/HTTP/Header,
    // "toJSONString" -> to/JSON/String, "readIOBuffer" -> read/IO/Buffer.
    // Without this, consecutive capitals would shatter into one-char
    // segments ("parseHTTPHeader" -> p-a-r-s-e-H-T-T-P... over-matching
    // even worse than a single leading segment).
    let mut ranges = Vec::new();
    let mut start = 0;
    let indices: Vec<(usize, char)> = name.char_indices().collect();
    for i in 1..indices.len() {
        let (idx, ch) = indices[i];
        if !ch.is_uppercase() {
            continue;
        }
        let prev_is_upper = indices[i - 1].1.is_uppercase();
        let next_is_lower = indices.get(i + 1).is_some_and(|&(_, c)| c.is_lowercase());
        if !prev_is_upper || next_is_lower {
            ranges.push((start, idx));
            start = idx;
        }
    }
    if !indices.is_empty() {
        ranges.push((start, name.len()));
    }
    ranges
}

/// Extract a multi-segment name prefix (e.g. "get_user" from "get_user_name",
/// "getUser" from "getUserName") used to suggest a placement module.
///
/// Only a prefix spanning **at least two** segments is meaningful enough to
/// suggest a placement — a single leading segment (e.g. "make" from
/// "make_relative") over-matches wildly, since unrelated functions across
/// the whole codebase commonly share a first word. Names with fewer than
/// three total segments can't produce a two-segment prefix while leaving a
/// remainder (a single word, or a two-word name where the "prefix" would be
/// the entire name), so they return an empty prefix and never fire W001.
pub fn extract_prefix(name: &str) -> String {
    let ranges = segment_ranges(name);
    if ranges.len() < 3 {
        return String::new();
    }
    name[ranges[0].0..ranges[1].1].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_params() {
        assert_eq!(count_params("fn foo()"), 0);
        assert_eq!(count_params("fn foo(a: i32)"), 1);
        assert_eq!(count_params("fn foo(a: i32, b: str)"), 2);
        assert_eq!(count_params("def bar(x, y, z)"), 3);
    }

    // E005 edge cases: zero params, many params, edge patterns
    #[test]
    fn test_count_params_zero() {
        assert_eq!(count_params("fn foo()"), 0);
        assert_eq!(count_params("def bar()"), 0);
        assert_eq!(count_params("func Baz()"), 0);
    }

    #[test]
    fn test_count_params_no_parens() {
        assert_eq!(count_params("fn foo"), 0);
        assert_eq!(count_params(""), 0);
    }

    #[test]
    fn test_count_params_many() {
        assert_eq!(count_params("fn f(a: i32, b: i32, c: i32, d: i32)"), 4);
        assert_eq!(count_params("def g(a, b, c, d, e)"), 5);
    }

    #[test]
    fn test_count_params_self_receiver() {
        // Rust method with self
        assert_eq!(count_params("fn method(&self, x: i32)"), 2);
    }

    #[test]
    fn test_count_call_args_empty() {
        assert_eq!(count_call_args("foo()"), 0);
    }

    #[test]
    fn test_count_call_args_no_parens() {
        assert_eq!(count_call_args("foo"), 0);
    }

    #[test]
    fn test_count_call_args_multiple() {
        assert_eq!(count_call_args("foo(a, b, c)"), 3);
    }

    #[test]
    fn test_extract_prefix() {
        assert_eq!(extract_prefix("x"), "");
    }

    #[test]
    fn test_extract_prefix_all_lowercase() {
        assert_eq!(extract_prefix("process"), "");
    }

    #[test]
    fn test_extract_prefix_single_segment_names_never_match() {
        // Common short verbs/names — never enough segments to produce a prefix.
        assert_eq!(extract_prefix("run"), "");
        assert_eq!(extract_prefix("main"), "");
        assert_eq!(extract_prefix("default"), "");
    }

    #[test]
    fn test_extract_prefix_snake_case_multi() {
        // 3+ segments: prefix is the first TWO segments, not just the first.
        assert_eq!(extract_prefix("get_user_name"), "get_user");
        assert_eq!(extract_prefix("get_user_profile_data"), "get_user");
    }

    #[test]
    fn test_extract_prefix_camel_case_multi() {
        assert_eq!(extract_prefix("getUserName"), "getUser");
        // Only two segments — no match, same rule as snake_case.
        assert_eq!(extract_prefix("handleRequest"), "");
    }

    #[test]
    fn test_extract_prefix_acronym_runs_stay_one_segment() {
        // Consecutive capitals are one segment (the acronym), not one
        // segment per letter — otherwise "parseHTTPHeader" would shatter
        // into p/a/r/s/e/H/T/T/P/... and over-match worse than the old
        // single-segment behavior. Standard camel tokenization: the run
        // splits right before its last capital when that capital is
        // followed by a lowercase letter (it belongs to the next word).
        assert_eq!(extract_prefix("parseHTTPHeader"), "parseHTTP");
        assert_eq!(extract_prefix("toJSONString"), "toJSON");
        assert_eq!(extract_prefix("readIOBuffer"), "readIO");

        // All-caps name: one giant acronym run = a single segment = no
        // multi-segment prefix possible.
        assert_eq!(extract_prefix("HTTP"), "");
    }

    #[test]
    fn test_extract_prefix_two_segment_names_never_match() {
        // Regression guard: these used to extract a single-segment prefix
        // ("make", "process") that over-matched wildly. Now they need a
        // third segment to leave room for a genuine 2-segment prefix.
        assert_eq!(extract_prefix("make_relative"), "");
        assert_eq!(extract_prefix("process_order"), "");
    }

    #[test]
    fn test_normalize_signature_whitespace_only_reformat_matches() {
        // rustfmt's common one-param-per-line wrap style — the `(`/`)`
        // glue directly to a newline that has no corresponding space in
        // the single-line form. (A trailing comma, if rustfmt adds one,
        // is a real content difference, not whitespace — out of scope
        // for whitespace normalization, and correctly still flagged.)
        let a = "fn foo(x: i32, y: i32) -> bool";
        let b = "fn foo(\n    x: i32,\n    y: i32\n) -> bool";
        assert_eq!(normalize_signature(a), normalize_signature(b));

        // prettier-style wrap without a trailing comma
        let c = "function foo(\n  x: number,\n  y: number\n): boolean";
        let d = "function foo(x: number, y: number): boolean";
        assert_eq!(normalize_signature(c), normalize_signature(d));

        // Extra/irregular spacing
        assert_eq!(
            normalize_signature("fn  foo( x:i32 )"),
            normalize_signature("fn foo(x:i32)")
        );

        // Leading/trailing whitespace
        assert_eq!(
            normalize_signature("  fn foo()  "),
            normalize_signature("fn foo()")
        );
    }

    #[test]
    fn test_normalize_signature_real_change_differs() {
        // Added parameter is a real signature change, not just reformatting
        assert_ne!(
            normalize_signature("fn foo(x: i32)"),
            normalize_signature("fn foo(x: i32, y: i32)")
        );
        // Type change is a real signature change
        assert_ne!(
            normalize_signature("fn foo(x: i32)"),
            normalize_signature("fn foo(x: i64)")
        );
        // Renamed function is a real signature change
        assert_ne!(
            normalize_signature("fn foo()"),
            normalize_signature("fn bar()")
        );
    }

    #[test]
    fn test_is_stub_file() {
        assert!(is_stub_file("src/models.pyi"));
        assert!(is_stub_file("types/global.d.ts"));
        assert!(!is_stub_file("src/models.py"));
        assert!(!is_stub_file("src/app.ts"));
    }

    #[test]
    fn test_is_test_file() {
        // Go
        assert!(is_test_file("pkg/handler_test.go"));
        assert!(!is_test_file("pkg/handler.go"));

        // Python
        assert!(is_test_file("tests/test_handler.py"));
        assert!(is_test_file("src/handler_test.py"));
        assert!(!is_test_file("src/handler.py"));
        assert!(!is_test_file("src/testing_utils.py")); // not a test file

        // TypeScript/JavaScript
        assert!(is_test_file("src/handler.test.ts"));
        assert!(is_test_file("src/handler.spec.ts"));
        assert!(is_test_file("src/handler.test.js"));
        assert!(is_test_file("src/handler.spec.tsx"));
        assert!(!is_test_file("src/handler.ts"));

        // Rust
        assert!(is_test_file("src/handler_test.rs"));
        assert!(is_test_file("src/tests.rs"));
        assert!(!is_test_file("src/handler.rs"));

        // Rust: plural `*_tests.rs` — this repo's own
        // `#[path = "..._tests.rs"] mod tests;` convention.
        assert!(is_test_file("crates/keel-core/src/sqlite_tests.rs"));
        assert!(is_test_file("engine_tests.rs"));
        assert!(is_test_file("utils_tests.rs"));
        // Suffix match, not substring: no underscore before "tests.rs".
        assert!(!is_test_file("contests.rs"));
        assert!(!is_test_file("src/testing_utils.rs")); // not a test file

        // Directory-based detection
        assert!(is_test_file("src/tests/helpers.py"));
        assert!(is_test_file("project/test/utils.go"));
        assert!(is_test_file("src/__tests__/handler.ts"));
        assert!(!is_test_file("src/contest/handler.py")); // "contest" != "test"

        // Top-level test directories (no parent path prefix)
        assert!(is_test_file("tests/helpers.py"));
        assert!(is_test_file("test/utils.go"));
        assert!(is_test_file("__tests__/handler.ts"));
    }
}
