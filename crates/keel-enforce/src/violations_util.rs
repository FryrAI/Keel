/// Check if a file path is a test file by language convention.
/// Patterns: *_test.go, test_*.py, *_test.py, *.test.ts, *.spec.ts,
/// *.test.js, *.spec.js, *_test.rs, tests.rs
pub fn is_test_file(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or(path);
    let basename = basename.rsplit('\\').next().unwrap_or(basename);

    // Go: *_test.go
    if basename.ends_with("_test.go") {
        return true;
    }
    // Python: test_*.py or *_test.py
    if basename.ends_with(".py") && (basename.starts_with("test_") || basename.ends_with("_test.py")) {
        return true;
    }
    // TypeScript/JavaScript: *.test.ts, *.spec.ts, *.test.js, *.spec.js, *.test.tsx, *.spec.tsx
    if basename.contains(".test.") || basename.contains(".spec.") {
        return true;
    }
    // Rust: *_test.rs or tests.rs
    if basename.ends_with("_test.rs") || basename == "tests.rs" {
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
    let Some(start) = name.find('(') else { return 0 };
    let Some(end) = name.rfind(')') else { return 0 };
    let args = &name[start + 1..end].trim();
    if args.is_empty() {
        return 0;
    }
    args.split(',').count()
}

/// Extract a name prefix (e.g., "handle" from "handleRequest").
pub fn extract_prefix(name: &str) -> String {
    // Split on camelCase or snake_case boundary
    if let Some(pos) = name.find('_') {
        return name[..pos].to_string();
    }
    // camelCase: find first lowercase->uppercase transition
    let chars: Vec<char> = name.chars().collect();
    for i in 1..chars.len() {
        if chars[i].is_uppercase() {
            return chars[..i].iter().collect();
        }
    }
    String::new()
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
        assert_eq!(extract_prefix("handleRequest"), "handle");
        assert_eq!(extract_prefix("process_order"), "process");
        assert_eq!(extract_prefix("x"), "");
    }

    #[test]
    fn test_extract_prefix_all_lowercase() {
        assert_eq!(extract_prefix("process"), "");
    }

    #[test]
    fn test_extract_prefix_snake_case_multi() {
        assert_eq!(extract_prefix("get_user_name"), "get");
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
    }
}
