//! Call-site argument-count extraction (issue #54): every language's call
//! reference carries the syntactic arity E005 compares against signatures.

use std::path::Path;

use super::*;
use crate::resolver::{ParseResult, ReferenceKind};

/// The `call_arity` recorded for the call reference named `name`.
fn arity_of(result: &ParseResult, name: &str) -> Option<u32> {
    result
        .references
        .iter()
        .find(|r| r.name == name && r.kind == ReferenceKind::Call)
        .unwrap_or_else(|| panic!("call reference `{name}` extracted"))
        .call_arity
}

/// Call references carry the syntactic argument count in every language —
/// the input E005 arity checking compares against the target's signature
/// (issue #54: references used to carry no count at all).
#[test]
fn call_references_carry_their_argument_count() {
    let mut parser = TreeSitterParser::new();

    let rust = "fn wire() {\n    plain(1, 2);\n    obj.method(1);\n    Vec::with_capacity(8);\n    none();\n}\n";
    let result = parser.parse_file("rust", Path::new("a.rs"), rust).unwrap();
    assert_eq!(arity_of(&result, "plain"), Some(2));
    assert_eq!(arity_of(&result, "obj.method"), Some(1));
    // A single-segment Rust path joins with `.` (existing extractor behavior).
    assert_eq!(arity_of(&result, "Vec.with_capacity"), Some(1));
    assert_eq!(arity_of(&result, "none"), Some(0));

    let py = "def wire():\n    plain(1, 2)\n    obj.method(1)\n    kw(a, b=2)\n";
    let result = parser.parse_file("python", Path::new("a.py"), py).unwrap();
    assert_eq!(arity_of(&result, "plain"), Some(2));
    assert_eq!(arity_of(&result, "obj.method"), Some(1));
    assert_eq!(arity_of(&result, "kw"), Some(2));

    let ts = "function wire() {\n    plain(1, 2);\n    obj.method(1);\n}\n";
    let result = parser
        .parse_file("typescript", Path::new("a.ts"), ts)
        .unwrap();
    assert_eq!(arity_of(&result, "plain"), Some(2));
    assert_eq!(arity_of(&result, "obj.method"), Some(1));

    let go = "package p\n\nfunc wire() {\n\tplain(1, 2)\n\tobj.Method(1)\n}\n";
    let result = parser.parse_file("go", Path::new("a.go"), go).unwrap();
    assert_eq!(arity_of(&result, "plain"), Some(2));
    assert_eq!(arity_of(&result, "obj.Method"), Some(1));
}

/// A splat/spread/variadic argument expands to an unknown count at runtime,
/// and a macro invocation has a token tree rather than an argument list — all
/// must record `None`, never a guessed count, so E005 skips them. One case
/// per grammar, so every language's uncountable shape stays pinned.
#[test]
fn uncountable_call_sites_record_no_arity() {
    let mut parser = TreeSitterParser::new();

    let py = "def wire():\n    splat(*args)\n    kwargs_splat(**kw)\n    gen(x for x in xs)\n";
    let result = parser.parse_file("python", Path::new("a.py"), py).unwrap();
    assert_eq!(arity_of(&result, "splat"), None, "`*args` splat");
    assert_eq!(arity_of(&result, "kwargs_splat"), None, "`**kw` splat");
    // A bare generator argument hangs a generator_expression on the
    // `arguments` field — its named children are comprehension internals,
    // not arguments, and must not be counted as two.
    assert_eq!(arity_of(&result, "gen"), None, "generator argument");

    let ts = "function wire() {\n    spread(...rest);\n    const q = sql`select ${x}`;\n}\n";
    let result = parser
        .parse_file("typescript", Path::new("a.ts"), ts)
        .unwrap();
    assert_eq!(arity_of(&result, "spread"), None, "`...rest` spread");
    // A tagged template's `arguments` field is a template_string — its
    // fragments are not an argument list.
    assert_eq!(arity_of(&result, "sql"), None, "tagged template");

    let go = "package p\n\nfunc wire() {\n\tspread(xs...)\n}\n";
    let result = parser.parse_file("go", Path::new("a.go"), go).unwrap();
    assert_eq!(arity_of(&result, "spread"), None, "Go `xs...` variadic");

    let rust = "fn wire() {\n    println!(\"{}\", 1);\n}\n";
    let result = parser.parse_file("rust", Path::new("a.rs"), rust).unwrap();
    assert_eq!(
        arity_of(&result, "println!"),
        None,
        "a macro token tree is not an argument list"
    );
}
