//! Boundary-literal capture tests (T1.4): a string literal naming a known
//! boundary symbol is a dispatch reference; every other string is dropped
//! inside the parser and never reaches the reference vector.
//!
//! Sources are raw strings, never `\`-continued ones: the continuation escape
//! eats the next line's leading whitespace, which silently un-indents Python
//! and turns whole blocks into `ERROR` nodes.

use std::path::Path;

use super::*;
use crate::resolver::ReferenceKind;

/// A parser that knows `names` as boundary symbols and nothing else.
fn parser_with_keys(names: &[&str]) -> TreeSitterParser {
    let mut parser = TreeSitterParser::new();
    parser.set_boundary_literals(Arc::new(
        names
            .iter()
            .map(|n| (*n).to_string())
            .collect::<HashSet<_>>(),
    ));
    parser
}

/// The lines carrying a `ReferenceKind::Literal` reference to `name`, sorted.
fn literal_lines(result: &crate::resolver::ParseResult, name: &str) -> Vec<u32> {
    let mut lines: Vec<u32> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Literal && r.name == name)
        .map(|r| r.line)
        .collect();
    lines.sort_unstable();
    lines
}

/// Every `ReferenceKind::Literal` reference in a parse, for negative assertions.
fn literal_names(result: &crate::resolver::ParseResult) -> Vec<&str> {
    result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Literal)
        .map(|r| r.name.as_str())
        .collect()
}

/// The Rust shape the feature exists for: a BAML function driven through a CLI
/// subprocess with its name as a string argument, plus the match-arm form.
#[test]
fn rust_literal_positions_are_captured() {
    let mut parser = parser_with_keys(&["PlanBerichtSection"]);
    let source = r#"fn drive(input: &str) -> String {
    run_baml("PlanBerichtSection", input)
}

fn route(kind: &str) -> u32 {
    match kind {
        "PlanBerichtSection" => 1,
        _ => 0,
    }
}
"#;
    let result = parser
        .parse_file("rust", Path::new("llm_impl.rs"), source)
        .unwrap();

    assert_eq!(
        literal_lines(&result, "PlanBerichtSection"),
        vec![2, 7],
        "call-argument and match-arm literals must both be captured"
    );
}

/// TypeScript: call argument, object key, and the `switch`-case analog of a
/// match arm.
#[test]
fn typescript_literal_positions_are_captured() {
    let mut parser = parser_with_keys(&["PlanBerichtSection"]);
    let source = r#"const table = { "PlanBerichtSection": 1 };
export function drive(input: string): string {
  return callBaml("PlanBerichtSection", input);
}
export function route(kind: string): number {
  switch (kind) {
    case "PlanBerichtSection":
      return 1;
    default:
      return 0;
  }
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("llm.ts"), source)
        .unwrap();

    assert_eq!(
        literal_lines(&result, "PlanBerichtSection"),
        vec![1, 3, 7],
        "object key, call argument and switch case must all be captured"
    );
}

/// Python: call argument, dict key, and `match`/`case` pattern.
#[test]
fn python_literal_positions_are_captured() {
    let mut parser = parser_with_keys(&["PlanBerichtSection"]);
    let source = r#"HANDLERS = {"PlanBerichtSection": 1}


def drive(text: str) -> str:
    return call_baml("PlanBerichtSection", text)


def route(kind: str) -> int:
    match kind:
        case "PlanBerichtSection":
            return 1
    return 0
"#;
    let result = parser
        .parse_file("python", Path::new("llm.py"), source)
        .unwrap();

    assert_eq!(
        literal_lines(&result, "PlanBerichtSection"),
        vec![1, 5, 10],
        "dict key, call argument and case pattern must all be captured"
    );
}

/// The filter that keeps the graph clean: a literal matching no known boundary
/// name produces NO reference at all — not a low-confidence one, not an edge
/// dropped later. Asserted on the reference vector, not on edge counts.
#[test]
fn literal_matching_no_boundary_name_produces_no_reference() {
    let mut parser = parser_with_keys(&["PlanBerichtSection"]);
    let source = r#"fn drive(input: &str) -> String {
    log("starting up");
    run_baml("NotABamlFunction", input)
}
"#;
    let result = parser
        .parse_file("rust", Path::new("llm_impl.rs"), source)
        .unwrap();

    assert!(
        literal_names(&result).is_empty(),
        "unknown literals must never reach the reference vector: {:?}",
        literal_names(&result)
    );
    // The surrounding calls are untouched.
    assert!(result
        .references
        .iter()
        .any(|r| r.name == "run_baml" && r.kind == ReferenceKind::Call));
}

/// With no boundary surface — the default for every repo that uses none — the
/// parser emits no literal references whatsoever.
#[test]
fn no_key_set_means_no_literal_references() {
    let mut parser = TreeSitterParser::new();
    let source = r#"fn drive(input: &str) -> String {
    run_baml("PlanBerichtSection", input)
}
"#;
    let result = parser
        .parse_file("rust", Path::new("llm_impl.rs"), source)
        .unwrap();

    assert!(literal_names(&result).is_empty());
}

/// A key is matched on its *exact* text: substrings, prefixed forms and case
/// variants are all misses.
#[test]
fn literal_matching_is_exact() {
    let mut parser = parser_with_keys(&["PlanBerichtSection"]);
    let source = r#"fn drive(input: &str) -> String {
    run_baml("planberichtsection", input);
    run_baml("PlanBerichtSectionV2", input);
    run_baml(b"PlanBerichtSection", input)
}
"#;
    let result = parser
        .parse_file("rust", Path::new("llm_impl.rs"), source)
        .unwrap();

    assert!(
        literal_names(&result).is_empty(),
        "only an exact text match is a boundary reference: {:?}",
        literal_names(&result)
    );
}
