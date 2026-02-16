// Tests for Python __all__ export list and public API handling (Spec 003 - Python Resolution)

use std::path::Path;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;

#[test]
#[ignore = "BUG: __all__ list parsing not implemented in tree-sitter layer"]
/// __all__ should define the public API of a module for star imports.
fn test_all_defines_public_api() {
    // Parsing __all__ = ['foo', 'Bar'] requires AST-level analysis of
    // assignment expressions, which is not in the tree-sitter query.
}

#[test]
#[ignore = "BUG: __all__ validation not implemented in tree-sitter layer"]
/// __all__ with names not defined in the module should produce a warning.
fn test_all_with_undefined_names() {
    // Requires cross-referencing __all__ entries against definitions,
    // a Tier 2 feature.
}

#[test]
/// Module without __all__ should use convention-based public API (no underscore prefix).
fn test_missing_all_uses_convention() {
    let resolver = PyResolver::new();
    let source = r#"
def process(data: str) -> str:
    return data

def _helper(x: int) -> int:
    return x + 1

def __internal():
    pass
"#;
    let path = Path::new("/project/module.py");
    let result = resolver.parse_file(path, source);

    assert_eq!(result.definitions.len(), 3);

    let process_def = result.definitions.iter().find(|d| d.name == "process").unwrap();
    let helper_def = result.definitions.iter().find(|d| d.name == "_helper").unwrap();
    let internal_def = result.definitions.iter().find(|d| d.name == "__internal").unwrap();

    assert!(process_def.is_public, "process() should be public");
    assert!(!helper_def.is_public, "_helper() should be private (underscore prefix)");
    assert!(!internal_def.is_public, "__internal() should be private (dunder prefix)");
}

#[test]
/// Type hints detection: fully typed function should report type_hints_present = true.
fn test_type_hints_fully_typed() {
    let resolver = PyResolver::new();
    let source = r#"
def greet(name: str, age: int) -> str:
    return f"Hello {name}, you are {age}"
"#;
    let path = Path::new("/project/typed.py");
    let result = resolver.parse_file(path, source);

    assert_eq!(result.definitions.len(), 1);
    assert!(
        result.definitions[0].type_hints_present,
        "Fully typed function should have type_hints_present = true"
    );
}

#[test]
/// Type hints detection: partially typed function (missing return) should report false.
fn test_type_hints_partial() {
    let resolver = PyResolver::new();
    let source = r#"
def greet(name: str):
    return f"Hello {name}"
"#;
    let path = Path::new("/project/partial.py");
    let result = resolver.parse_file(path, source);

    assert_eq!(result.definitions.len(), 1);
    assert!(
        !result.definitions[0].type_hints_present,
        "Function missing return type hint should have type_hints_present = false"
    );
}

#[test]
/// Type hints detection: untyped function should report false.
fn test_type_hints_untyped() {
    let resolver = PyResolver::new();
    let source = r#"
def greet(name):
    return f"Hello {name}"
"#;
    let path = Path::new("/project/untyped.py");
    let result = resolver.parse_file(path, source);

    assert_eq!(result.definitions.len(), 1);
    assert!(
        !result.definitions[0].type_hints_present,
        "Untyped function should have type_hints_present = false"
    );
}

#[test]
#[ignore = "BUG: __all__ concatenation parsing not implemented"]
/// __all__ should be parsed even when defined with concatenation.
fn test_all_with_concatenation() {
    // Parsing __all__ = ['foo'] + ['bar'] requires expression evaluation.
}

#[test]
#[ignore = "BUG: __all__ change tracking not implemented"]
/// __all__ changes should trigger re-evaluation of dependent imports.
fn test_all_change_triggers_reevaluation() {
    // Requires incremental dependency tracking for __all__ changes.
}

#[test]
#[ignore = "BUG: dynamic __all__ detection not implemented"]
/// Dynamic __all__ (computed at runtime) should be marked as unresolvable.
fn test_dynamic_all_unresolvable() {
    // Requires static analysis of __all__ expressions to detect dynamic cases.
}
