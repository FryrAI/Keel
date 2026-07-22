//! Value-position capture tests: function-as-value references the
//! direct-argument pattern cannot see (W005 must not call these dead).

use std::path::Path;

use super::*;
use crate::resolver::ReferenceKind;

/// Collect the names of every `ReferenceKind::Value` reference in a parse.
fn value_ref_names(result: &crate::resolver::ParseResult) -> Vec<&str> {
    result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Value)
        .map(|r| r.name.as_str())
        .collect()
}

/// Python function-as-value positions the direct-argument pattern cannot see:
/// keyword arguments, dict/list/tuple/set values, returns, bare decorators and
/// alias assignments. Each is real usage, so W005 must not call them dead.
#[test]
fn python_nested_value_positions_are_captured() {
    let mut parser = TreeSitterParser::new();
    let source = "\
sorted(xs, key=sort_key_target)\n\
HANDLERS = {\"evt\": dict_dispatch_target}\n\
LIST_STEPS = [list_target]\n\
TUPLE_STEPS = (tuple_target,)\n\
SET_STEPS = {set_target}\n\
ALIAS = assign_target\n\
\n\
def outer():\n\
    return return_target\n\
\n\
@bare_decorator_target\n\
def decorated():\n\
    pass\n\
\n\
@call_decorator_target(\"evt\")\n\
def decorated_call():\n\
    pass\n";
    let result = parser
        .parse_file("python", Path::new("wire.py"), source)
        .unwrap();
    let values = value_ref_names(&result);

    for name in [
        "sort_key_target",
        "dict_dispatch_target",
        "list_target",
        "tuple_target",
        "set_target",
        "assign_target",
        "return_target",
        "bare_decorator_target",
    ] {
        assert!(values.contains(&name), "{name} must be a Value reference");
    }

    // The call form of a decorator stays a Call reference — the new bare-name
    // pattern must not swallow it.
    assert!(
        result
            .references
            .iter()
            .any(|r| r.name == "call_decorator_target" && r.kind == ReferenceKind::Call),
        "`@decorator(\"evt\")` must still be a Call reference"
    );
    // A dict key is never a value position.
    assert!(
        !values.contains(&"evt"),
        "dict keys are not value references"
    );
}

/// TypeScript function-as-value positions: object-literal values, shorthand
/// properties, array elements, returns and alias declarations.
#[test]
fn typescript_nested_value_positions_are_captured() {
    let mut parser = TreeSitterParser::new();
    let source = "\
const table = { evt: tableHandler };\n\
const shorthand = { stringLookupTarget };\n\
const steps = [arrayTarget];\n\
const alias = aliasTarget;\n\
function pick() {\n\
  return returnTarget;\n\
}\n";
    let result = parser
        .parse_file("typescript", Path::new("wire.ts"), source)
        .unwrap();
    let values = value_ref_names(&result);

    for name in [
        "tableHandler",
        "stringLookupTarget",
        "arrayTarget",
        "aliasTarget",
        "returnTarget",
    ] {
        assert!(values.contains(&name), "{name} must be a Value reference");
    }
    // Keys are not value positions.
    assert!(
        !values.contains(&"evt"),
        "object keys are not value references"
    );
}

/// Go function-as-value positions: composite-literal elements (bare and keyed),
/// returns, and `:=` / `=` / `var` right-hand sides.
#[test]
fn go_nested_value_positions_are_captured() {
    let mut parser = TreeSitterParser::new();
    let source = "\
package main\n\
\n\
var jobs = []func(){tableJob}\n\
var routes = map[string]func(){\"evt\": keyedJob}\n\
var aliased = varTarget\n\
\n\
func pick() func() {\n\
\treturn returnTarget\n\
}\n\
\n\
func wire() {\n\
\tshort := shortTarget\n\
\tshort = assignTarget\n\
\t_ = short\n\
}\n";
    let result = parser
        .parse_file("go", Path::new("wire.go"), source)
        .unwrap();
    let values = value_ref_names(&result);

    for name in [
        "tableJob",
        "keyedJob",
        "varTarget",
        "returnTarget",
        "shortTarget",
        "assignTarget",
    ] {
        assert!(values.contains(&name), "{name} must be a Value reference");
    }
}

/// Rust function-as-value positions: struct-literal field values, `&ident`
/// arguments, `let` aliases, array elements and returns.
#[test]
fn rust_nested_value_positions_are_captured() {
    let mut parser = TreeSitterParser::new();
    let source = "\
fn wire() {\n\
    let holder = Holder { cb: struct_field_target };\n\
    register(&borrowed_target);\n\
    let alias = let_target;\n\
    let table = [array_target];\n\
}\n\
\n\
fn pick() -> fn() {\n\
    return return_target;\n\
}\n";
    let result = parser
        .parse_file("rust", Path::new("wire.rs"), source)
        .unwrap();
    let values = value_ref_names(&result);

    for name in [
        "struct_field_target",
        "borrowed_target",
        "let_target",
        "array_target",
        "return_target",
    ] {
        assert!(values.contains(&name), "{name} must be a Value reference");
    }
    // Field names are not value positions.
    assert!(!values.contains(&"cb"), "struct field names are not values");
}
