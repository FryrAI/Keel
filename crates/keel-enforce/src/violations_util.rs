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

/// Compute one disambiguated hash for a definition under an arbitrary salt.
///
/// [`definition_hashes`] covers the two identities `keel map` assigns (plain,
/// and file-path-salted). A file holding three or more identical same-named
/// definitions needs more than two distinct identities, so the engine's
/// re-baseline walks an ordinal (`"<file>#2"`, `"<file>#3"`, …) through this.
/// Off the hot path — it re-normalizes the body — and only reached once a
/// collision is already proven.
pub fn definition_hash_salted(def: &keel_parsers::resolver::Definition, salt: &str) -> String {
    keel_core::hash::compute_hash_disambiguated(
        &def.signature,
        &def.body_for_hash(),
        def.docstring.as_deref().unwrap_or(""),
        salt,
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
/// Patterns: *_test.go, test_*.py, *_test.py, conftest.py, *.test.ts,
/// *.spec.ts, *.test.js, *.spec.js, *_test.rs, *_tests.rs, *_tests_*.rs,
/// tests.rs
///
/// Now that `Definition::in_test_context` marks test symbols precisely per
/// grammar (Rust/Python/TS in the tree-sitter walk, Go in its Tier-2 pass),
/// this coarse filename check has narrowed to a fallback: it is the only signal
/// for a file whose `parse_file` returned no definitions, and a belt-and-braces
/// safety net for test-support code the per-definition AST rules do not
/// individually catch — pytest `@pytest.fixture` helpers with non-`test_`
/// names, Go/TS test utilities. It is also still the sole test signal for the
/// store-based cross-file filters, whose `GraphNode`s carry no context flags.
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
    // Python: test_*.py, *_test.py, or the pytest fixture-root conftest.py —
    // fixtures there are consumed via their parameter NAME by tests in other
    // files, so the graph shows zero callers even though pytest invokes them.
    if basename == "conftest.py"
        || (basename.ends_with(".py")
            && (basename.starts_with("test_") || basename.ends_with("_test.py")))
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
    // Also `*_tests_*.rs` — the `engine_tests_*.rs` split-file convention
    // (`engine_tests_e004_misc.rs`, `engine_tests_economy.rs`). `_tests_`
    // (underscore on both sides) stays precise: "contests_utils.rs" has
    // "ntests_", no leading underscore, so it does not match.
    if basename.ends_with("_test.rs")
        || basename.ends_with("_tests.rs")
        || basename == "tests.rs"
        || basename.contains("_tests_")
    {
        return true;
    }
    false
}

/// Maximum characters a parenthesized span may cover before it is treated as
/// unbalanced prose rather than an argument list. Bounds the plan scanner,
/// which runs over free text where an opening paren may never close.
pub(crate) const MAX_CLAIM_SPAN: usize = 600;

/// A signature (or a plan's call claim) reduced to the two things keel
/// compares: how many arguments it takes, and whether it declares a return.
pub(crate) struct ParsedSig {
    /// Parameter count with any receiver removed.
    pub(crate) arity: usize,
    /// Whether an explicit `-> T` follows the parameter list.
    pub(crate) has_return: bool,
}

/// True for characters that may appear inside an identifier.
pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Index of the `)` closing the `(` at `open`, or `None` when the span is
/// unbalanced, longer than `MAX_CLAIM_SPAN`, or crosses a blank line.
pub(crate) fn match_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut prev = '\0';
    let mut newlines = 0usize;
    for (offset, &ch) in chars[open..].iter().enumerate() {
        if offset > MAX_CLAIM_SPAN {
            return None;
        }
        if let Some(q) = in_str {
            if ch == q && prev != '\\' {
                in_str = None;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' | '\'' | '`' => in_str = Some(ch),
            '\n' => {
                newlines += 1;
                if newlines > 6 {
                    return None;
                }
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
        prev = ch;
    }
    None
}

/// Split an argument list on top-level commas, tracking `()`, `[]`, `{}`,
/// generic `<>` (only when the `<` follows an identifier, so `a < b` is not a
/// bracket) and string literals.
pub(crate) fn split_top_level(args: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut round, mut square, mut curly, mut angle) = (0i32, 0i32, 0i32, 0i32);
    let mut in_str: Option<char> = None;
    let mut cur = String::new();
    let mut prev = '\0';
    for ch in args.chars() {
        if let Some(q) = in_str {
            cur.push(ch);
            if ch == q && prev != '\\' {
                in_str = None;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                in_str = Some(ch);
                cur.push(ch);
            }
            '(' => {
                round += 1;
                cur.push(ch);
            }
            ')' => {
                round -= 1;
                cur.push(ch);
            }
            '[' => {
                square += 1;
                cur.push(ch);
            }
            ']' => {
                square -= 1;
                cur.push(ch);
            }
            '{' => {
                curly += 1;
                cur.push(ch);
            }
            '}' => {
                curly -= 1;
                cur.push(ch);
            }
            '<' if is_ident_char(prev) => {
                angle += 1;
                cur.push(ch);
            }
            '>' if angle > 0 && prev != '-' && prev != '=' => {
                angle -= 1;
                cur.push(ch);
            }
            ',' if round == 0 && square == 0 && curly == 0 && angle == 0 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
        prev = ch;
    }
    parts.push(cur);
    parts
}

/// Drop a leading receiver parameter (`self`, `&self`, `&mut self`, `cls`,
/// `this`): it is never written at the call site, so counting it would make
/// every method call look like it is one argument short.
pub(crate) fn strip_receiver(parts: &mut Vec<String>) {
    let is_receiver = parts.first().is_some_and(|first| {
        let t = first
            .trim()
            .trim_start_matches('&')
            .trim()
            .trim_start_matches("mut ")
            .trim();
        let head = t
            .split(|c: char| c == ':' || c.is_whitespace())
            .next()
            .unwrap_or("");
        matches!(head, "self" | "cls" | "this")
    });
    if is_receiver {
        parts.remove(0);
    }
}

/// Countable argument count, or `None` when the list is variadic (`*args`,
/// `...rest`), defaulted (`=`), optional (`?`) or elided (`...`) — all cases
/// where "the plan says N" and "the code takes N" are not comparable.
pub(crate) fn countable_arity(parts: &[String]) -> Option<usize> {
    let trimmed: Vec<&str> = parts.iter().map(|p| p.trim()).collect();
    if trimmed.len() == 1 && trimmed[0].is_empty() {
        return Some(0);
    }
    for p in &trimmed {
        if p.is_empty()
            || p.starts_with('*')
            || p.starts_with("...")
            || p.starts_with('…')
            || p.contains('=')
            || p.contains('?')
        {
            return None;
        }
    }
    Some(trimmed.len())
}

/// Reduce a signature (`name(params) -> ret`) to arity and return presence.
/// `None` when there is no parameter list or the parameters are not countable.
pub(crate) fn parse_signature(sig: &str) -> Option<ParsedSig> {
    let chars: Vec<char> = sig.chars().collect();
    let open = chars.iter().position(|&c| c == '(')?;
    let close = match_paren(&chars, open)?;
    let args: String = chars[open + 1..close].iter().collect();
    let mut parts = split_top_level(&args);
    strip_receiver(&mut parts);
    let arity = countable_arity(&parts)?;
    let tail: String = chars[close + 1..].iter().collect();
    Some(ParsedSig {
        arity,
        has_return: tail.contains("->"),
    })
}

/// Count parameters from a signature string. Returns 0 if unable to parse.
///
/// Delegates to `parse_signature`, so a nested generic (`HashMap<K, V>` is one
/// parameter, not two) and a receiver (`&self`/`cls`/`this`, never written at
/// the call site) are counted the way E005's caller-side count sees them.
/// A parameter list the precise parser declines to count at all — variadic,
/// defaulted or optional — falls back to the naive comma split, keeping those
/// signatures exactly as E005 compared them before.
pub fn count_params(sig: &str) -> usize {
    if let Some(parsed) = parse_signature(sig) {
        return parsed.arity;
    }
    let Some(start) = sig.find('(') else { return 0 };
    let Some(end) = sig.find(')') else { return 0 };
    let params = sig[start + 1..end].trim();
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
        // The receiver is never written at the call site, so it is not counted:
        // `obj.method(x)` passes one argument and the definition takes one.
        assert_eq!(count_params("fn method(&self, x: i32)"), 1);
        assert_eq!(count_params("def method(self, x)"), 1);
        assert_eq!(count_params("fn method(&mut self)"), 0);
    }

    #[test]
    fn test_count_params_nested_generics_are_one_param() {
        // A comma inside `<>` / `()` / `[]` does not start a new parameter.
        assert_eq!(count_params("fn f(m: HashMap<String, i32>)"), 1);
        assert_eq!(count_params("fn f(m: HashMap<String, i32>, n: u8)"), 2);
        assert_eq!(count_params("fn f(cb: fn(a: i32, b: i32) -> i32)"), 1);
    }

    #[test]
    fn test_count_params_uncountable_lists_keep_the_naive_count() {
        // Variadic/defaulted/optional lists are not comparable to a call site;
        // the fallback keeps E005's pre-existing behavior for them.
        assert_eq!(count_params("def f(a, b=1)"), 2);
        assert_eq!(count_params("def f(*args)"), 1);
        assert_eq!(count_params("function f(a: number, b?: number)"), 2);
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

        // Rust: `*_tests_*.rs` split-file convention (engine_tests_*.rs) —
        // full paths and bare basenames.
        assert!(is_test_file(
            "crates/keel-enforce/src/engine_tests_e004_misc.rs"
        ));
        assert!(is_test_file("engine_tests_economy.rs"));
        assert!(is_test_file("engine_tests_batch_suppress.rs"));
        // `_tests_` needs an underscore on BOTH sides: "contests_utils.rs"
        // has "ntests_" (no leading underscore), so it does not match.
        assert!(!is_test_file("contests_utils.rs"));

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

    #[test]
    fn test_is_test_file_conftest() {
        // pytest's fixture-root file: fixtures are consumed by parameter name
        // in other test files, so it must be recognized as a test file even
        // though its basename matches none of the test_*/{_test} patterns.
        assert!(is_test_file("conftest.py"));
        assert!(is_test_file("app/tests/conftest.py"));
        // Exact basename match only — a name that merely contains "conftest"
        // is ordinary production code.
        assert!(!is_test_file("myconftest.py"));
    }
}
