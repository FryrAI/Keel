// Tests for Python __all__ export list and public API handling (Spec 003 - Python Resolution)

use keel_core::types::NodeKind;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use std::path::Path;

#[test]
/// __all__ should define the public API of a module for star imports.
fn test_all_defines_public_api() {
    let resolver = PyResolver::new();
    let source = r#"
__all__ = ['foo', 'Bar']

def foo():
    pass

def bar():
    pass

class Bar:
    pass

class Baz:
    pass
"#;
    let path = Path::new("/project/api.py");
    let result = resolver.parse_file(path, source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    let foo = defs.iter().find(|d| d.name == "foo").unwrap();
    let bar = defs.iter().find(|d| d.name == "bar").unwrap();
    let bar_cls = defs.iter().find(|d| d.name == "Bar").unwrap();
    let baz = defs.iter().find(|d| d.name == "Baz").unwrap();

    assert!(foo.is_public, "foo should be public (__all__ listed)");
    assert!(!bar.is_public, "bar should be private (not in __all__)");
    assert!(bar_cls.is_public, "Bar should be public (__all__ listed)");
    assert!(!baz.is_public, "Baz should be private (not in __all__)");
}

#[test]
/// __all__ with names not defined in the module should not crash.
fn test_all_with_undefined_names() {
    let resolver = PyResolver::new();
    let source = r#"
__all__ = ['foo', 'nonexistent']

def foo():
    pass

def bar():
    pass
"#;
    let path = Path::new("/project/partial_all.py");
    let result = resolver.parse_file(path, source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    let foo = defs.iter().find(|d| d.name == "foo").unwrap();
    let bar = defs.iter().find(|d| d.name == "bar").unwrap();

    assert!(foo.is_public, "foo should be public (__all__ listed)");
    assert!(!bar.is_public, "bar should be private (not in __all__)");
    // 'nonexistent' in __all__ doesn't crash — it just has no matching def
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
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    assert_eq!(defs.len(), 3);

    let process_def = defs.iter().find(|d| d.name == "process").unwrap();
    let helper_def = defs.iter().find(|d| d.name == "_helper").unwrap();
    let internal_def = defs.iter().find(|d| d.name == "__internal").unwrap();

    assert!(process_def.is_public, "process() should be public");
    assert!(
        !helper_def.is_public,
        "_helper() should be private (underscore prefix)"
    );
    assert!(
        !internal_def.is_public,
        "__internal() should be private (dunder prefix)"
    );
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
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    assert_eq!(defs.len(), 1);
    assert!(
        defs[0].type_hints_present,
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
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    assert_eq!(defs.len(), 1);
    assert!(
        !defs[0].type_hints_present,
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
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    assert_eq!(defs.len(), 1);
    assert!(
        !defs[0].type_hints_present,
        "Untyped function should have type_hints_present = false"
    );
}

#[test]
/// __all__ with concatenation falls back to convention-based visibility.
fn test_all_with_concatenation() {
    let resolver = PyResolver::new();
    let source = r#"
__all__ = ['foo'] + ['bar']

def foo():
    pass

def _private():
    pass
"#;
    let path = Path::new("/project/concat_all.py");
    let result = resolver.parse_file(path, source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    let foo = defs.iter().find(|d| d.name == "foo").unwrap();
    let priv_fn = defs.iter().find(|d| d.name == "_private").unwrap();

    // Concatenation is dynamic — falls back to underscore convention
    assert!(foo.is_public, "foo should be public (no underscore)");
    assert!(
        !priv_fn.is_public,
        "_private should be private (underscore)"
    );
}

#[test]
/// Re-parsing with different __all__ should update visibility.
fn test_all_change_triggers_reevaluation() {
    let resolver = PyResolver::new();
    let path = Path::new("/project/changing.py");

    // First parse: only foo is public
    let source1 = r#"
__all__ = ['foo']

def foo():
    pass

def bar():
    pass
"#;
    let result1 = resolver.parse_file(path, source1);
    let defs1: Vec<_> = result1
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert!(defs1.iter().find(|d| d.name == "foo").unwrap().is_public);
    assert!(!defs1.iter().find(|d| d.name == "bar").unwrap().is_public);

    // Second parse: both foo and bar are public
    let source2 = r#"
__all__ = ['foo', 'bar']

def foo():
    pass

def bar():
    pass
"#;
    let result2 = resolver.parse_file(path, source2);
    let defs2: Vec<_> = result2
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert!(defs2.iter().find(|d| d.name == "foo").unwrap().is_public);
    assert!(defs2.iter().find(|d| d.name == "bar").unwrap().is_public);
}

#[test]
/// Dynamic __all__ (computed at runtime) falls back to convention.
fn test_dynamic_all_unresolvable() {
    let resolver = PyResolver::new();
    let source = r#"
__all__ = get_exports()

def foo():
    pass

def _private():
    pass
"#;
    let path = Path::new("/project/dynamic_all.py");
    let result = resolver.parse_file(path, source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    let foo = defs.iter().find(|d| d.name == "foo").unwrap();
    let priv_fn = defs.iter().find(|d| d.name == "_private").unwrap();

    // Dynamic __all__ is unresolvable — falls back to underscore convention
    assert!(foo.is_public, "foo should be public (no underscore)");
    assert!(
        !priv_fn.is_public,
        "_private should be private (underscore)"
    );
}
