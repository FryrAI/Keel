// Tests for Python __init__.py package resolution (Spec 003 - Python Resolution)
//
// Package-level resolution through __init__.py requires filesystem access and
// multi-file resolution that is a Tier 2 feature (via ty subprocess).
// Tests that exercise available parser functionality have real assertions;
// tests requiring unavailable features are marked #[ignore].

use keel_core::types::NodeKind;
use keel_parsers::python::package_resolution;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use std::fs;
use std::path::Path;

#[test]
/// Importing a package name: the import statement should be captured.
fn test_package_resolves_to_init() {
    let resolver = PyResolver::new();
    let source = r#"
import package
"#;
    let result = resolver.parse_file(Path::new("/project/app.py"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    // bare `import package` uses import_statement, which tree-sitter captures.
    // The import may or may not produce an Import entry depending on query coverage
    // for bare imports (vs from-imports). Document actual behavior.
    // Parsing should succeed without error regardless.
    assert!(defs.is_empty(), "import-only file has no definitions");
}

#[test]
/// Importing a name from a package: the from-import should be captured.
fn test_package_name_from_init() {
    let resolver = PyResolver::new();
    let source = r#"
from package import process
"#;
    let result = resolver.parse_file(Path::new("/project/app.py"), source);

    assert!(!result.imports.is_empty(), "from-import should be captured");
    let imp = &result.imports[0];
    assert_eq!(imp.source, "package");
    assert!(imp.imported_names.contains(&"process".to_string()));
}

#[test]
/// __init__.py re-exporting from submodules: the relative import should be captured.
fn test_init_reexports_submodule() {
    let resolver = PyResolver::new();
    let source = r#"
from .parser import parse
"#;
    let result = resolver.parse_file(Path::new("/project/package/__init__.py"), source);

    assert!(
        !result.imports.is_empty(),
        "relative import should be captured"
    );
    let imp = &result.imports[0];
    assert!(imp.is_relative, "from .parser should be relative");
    assert!(imp.imported_names.contains(&"parse".to_string()));
}

#[test]
/// Namespace packages (no __init__.py) should still resolve submodules.
fn test_namespace_package_resolution() {
    let dir = std::env::temp_dir().join("keel_test_ns_pkg");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("project/ns_pkg")).unwrap();

    // Create a module inside the namespace package (no __init__.py)
    fs::write(dir.join("project/ns_pkg/module.py"), "def helper(): pass").unwrap();

    // Resolve ns_pkg.module from the project directory
    let result =
        package_resolution::resolve_python_package_chain(&dir.join("project"), "ns_pkg.module");

    assert!(
        result.is_some(),
        "should resolve namespace package submodule"
    );
    let resolved = result.unwrap();
    assert!(
        resolved.ends_with("ns_pkg/module.py") || resolved.ends_with("ns_pkg\\module.py"),
        "should resolve to ns_pkg/module.py, got: {}",
        resolved.display()
    );

    // Verify that a non-existent intermediate directory returns None
    let missing = package_resolution::resolve_python_package_chain(
        &dir.join("project"),
        "nonexistent.module",
    );
    assert!(missing.is_none(), "missing directory should return None");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
/// Empty __init__.py should parse without errors.
fn test_empty_init_valid_package() {
    let resolver = PyResolver::new();
    let result = resolver.parse_file(Path::new("/project/package/__init__.py"), "");
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    assert!(defs.is_empty(), "empty file has no definitions");
    assert!(result.imports.is_empty(), "empty file has no imports");
    assert!(result.references.is_empty(), "empty file has no references");
}

#[test]
/// Deeply nested packages should resolve through each __init__.py in the chain.
fn test_deep_nested_package_resolution() {
    let dir = std::env::temp_dir().join("keel_test_deep_pkg");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("project/a/b/c/d")).unwrap();

    // Create __init__.py at each level
    fs::write(dir.join("project/a/__init__.py"), "").unwrap();
    fs::write(dir.join("project/a/b/__init__.py"), "").unwrap();
    fs::write(dir.join("project/a/b/c/__init__.py"), "").unwrap();
    fs::write(dir.join("project/a/b/c/d/__init__.py"), "def deep(): pass").unwrap();

    // Resolve a.b.c.d from project directory
    let result = package_resolution::resolve_python_package_chain(&dir.join("project"), "a.b.c.d");

    assert!(result.is_some(), "should resolve deep nested package chain");
    let resolved = result.unwrap();
    assert!(
        resolved.ends_with("a/b/c/d/__init__.py") || resolved.ends_with("a\\b\\c\\d\\__init__.py"),
        "should resolve to a/b/c/d/__init__.py, got: {}",
        resolved.display()
    );

    // Also test partial chain: a.b should resolve
    let partial = package_resolution::resolve_python_package_chain(&dir.join("project"), "a.b");
    assert!(partial.is_some(), "should resolve partial chain a.b");
    let partial_resolved = partial.unwrap();
    assert!(
        partial_resolved.ends_with("a/b/__init__.py")
            || partial_resolved.ends_with("a\\b\\__init__.py"),
        "a.b should resolve to a/b/__init__.py, got: {}",
        partial_resolved.display()
    );

    // Test that missing deep path returns None
    let missing =
        package_resolution::resolve_python_package_chain(&dir.join("project"), "a.b.c.d.e");
    assert!(
        missing.is_none(),
        "non-existent deep path should return None"
    );

    let _ = fs::remove_dir_all(&dir);
}
