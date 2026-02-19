//! SCIP symbol string parsing.
//!
//! Format: `scheme manager package_name version descriptor_path`
//!
//! Descriptor suffix characters: `#` (term), `.` (type/namespace),
//! `()` (method), `[]` (type parameter).
//!
//! Reference: <https://github.com/sourcegraph/scip/blob/main/docs/reference.md>

/// A parsed SCIP symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScipSymbol {
    pub scheme: String,
    pub manager: String,
    pub package_name: String,
    pub version: String,
    /// `/`-split path components of the descriptor, each retaining its suffix.
    pub descriptors: Vec<String>,
    /// Raw descriptor path, used by `symbol_name` for right-to-left scanning.
    pub(crate) descriptor_path: String,
}

/// Parse a SCIP symbol string into a `ScipSymbol`.
///
/// Returns `None` for empty strings or strings missing any of the four
/// required header tokens or a non-empty descriptor path.
pub fn parse_symbol(symbol_str: &str) -> Option<ScipSymbol> {
    if symbol_str.is_empty() {
        return None;
    }
    let mut parts = symbol_str.splitn(5, ' ');
    let scheme = parts.next()?.to_owned();
    let manager = parts.next()?.to_owned();
    let package_name = parts.next()?.to_owned();
    let version = parts.next()?.to_owned();
    let descriptor_path = parts.next()?.to_owned();

    if descriptor_path.is_empty() {
        return None;
    }

    let descriptors: Vec<String> = descriptor_path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .collect();

    Some(ScipSymbol { scheme, manager, package_name, version, descriptors, descriptor_path })
}

/// Extract the simple function/class name from a SCIP symbol.
///
/// Scans the descriptor path from the right: skips trailing suffix/separator
/// characters (`# . ) ( ] [`), then collects name characters until the next
/// suffix or path separator (`/`).
///
/// Examples:
///   `src/index.ts/myFunc#`           -> `myFunc`
///   `src/index.ts/MyClass#render().` -> `render`
///   `src/foo.ts/Container#T[]`       -> `T`
pub fn symbol_name(symbol: &ScipSymbol) -> String {
    let path = &symbol.descriptor_path;
    if path.is_empty() {
        return String::new();
    }

    let chars: Vec<char> = path.chars().collect();
    let len = chars.len();

    // Skip trailing suffix/separator characters.
    let mut end = len;
    while end > 0 && is_suffix_or_sep(chars[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        return String::new();
    }

    // Collect name characters back to the previous suffix or '/'.
    let mut start = end;
    while start > 0 {
        let c = chars[start - 1];
        if is_suffix_or_sep(c) || c == '/' {
            break;
        }
        start -= 1;
    }

    chars[start..end].iter().collect()
}

fn is_suffix_or_sep(c: char) -> bool {
    matches!(c, '#' | '.' | ')' | '(' | ']' | '[')
}

/// Return `true` if the symbol's simple name matches `name` (case-sensitive).
pub fn symbol_matches_name(symbol: &ScipSymbol, name: &str) -> bool {
    symbol_name(symbol) == name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_returns_none() {
        assert!(parse_symbol("").is_none());
    }

    #[test]
    fn test_parse_too_few_parts_returns_none() {
        assert!(parse_symbol("scip-typescript npm pkg").is_none());
    }

    #[test]
    fn test_parse_valid_symbol_fields() {
        let sym = parse_symbol("scip-typescript npm my-pkg 1.0.0 src/index.ts/myFunc#")
            .expect("should parse");
        assert_eq!(sym.scheme, "scip-typescript");
        assert_eq!(sym.manager, "npm");
        assert_eq!(sym.package_name, "my-pkg");
        assert_eq!(sym.version, "1.0.0");
        assert!(!sym.descriptors.is_empty());
    }

    #[test]
    fn test_symbol_name_term() {
        let sym = parse_symbol("scip-typescript npm pkg 1.0.0 src/index.ts/myFunc#").unwrap();
        assert_eq!(symbol_name(&sym), "myFunc");
    }

    #[test]
    fn test_symbol_name_method() {
        let sym = parse_symbol("scip-typescript npm pkg 1.0.0 src/index.ts/MyClass#render().")
            .unwrap();
        assert_eq!(symbol_name(&sym), "render");
    }

    #[test]
    fn test_symbol_name_type_param() {
        let sym = parse_symbol("scip-typescript npm pkg 1.0.0 src/foo.ts/Container#T[]").unwrap();
        assert_eq!(symbol_name(&sym), "T");
    }

    #[test]
    fn test_symbol_name_namespace() {
        let sym = parse_symbol("scip-go go pkg v1.0.0 github.com/foo/bar.").unwrap();
        assert_eq!(symbol_name(&sym), "bar");
    }

    #[test]
    fn test_symbol_name_empty_path() {
        let sym = ScipSymbol {
            scheme: "s".into(), manager: "m".into(),
            package_name: "p".into(), version: "v".into(),
            descriptors: vec![], descriptor_path: String::new(),
        };
        assert_eq!(symbol_name(&sym), "");
    }

    #[test]
    fn test_symbol_matches_name() {
        let sym = parse_symbol("scip-python python pkg 3.10 src/app.py/handle_request#").unwrap();
        assert!(symbol_matches_name(&sym, "handle_request"));
        assert!(!symbol_matches_name(&sym, "other_func"));
    }
}
