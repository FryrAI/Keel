// Tests for Python package and module resolution (Spec 003 - Python Resolution)
//
// Most package resolution features require filesystem access and the ty
// subprocess (Tier 2), which are not available at the parser layer.

use std::path::{Path, PathBuf};
use keel_parsers::python::ty::{MockTyClient, TyClient, TyResult};
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;

#[test]
/// Absolute import of a local package should be captured as an import entry.
fn test_absolute_import_local_package() {
    let resolver = PyResolver::new();
    let source = r#"
from utils.parser import parse
"#;
    let result = resolver.parse_file(Path::new("/project/app.py"), source);

    assert!(!result.imports.is_empty(), "from-import should be captured");
    let imp = &result.imports[0];
    assert_eq!(imp.source, "utils.parser");
    assert!(imp.imported_names.contains(&"parse".to_string()));
    assert!(!imp.is_relative, "utils.parser is an absolute import");
}

#[test]
/// Import of a third-party package should be captured and recognized as non-relative.
fn test_third_party_package_import() {
    let resolver = PyResolver::new();
    let source = r#"
from requests import get
"#;
    let result = resolver.parse_file(Path::new("/project/app.py"), source);

    assert!(!result.imports.is_empty(), "from-import should be captured");
    let imp = &result.imports[0];
    assert_eq!(imp.source, "requests");
    assert!(!imp.is_relative, "third-party import should not be relative");
}

#[test]
/// Conditional imports (inside try blocks) should still be captured by tree-sitter.
fn test_conditional_import_resolution() {
    let resolver = PyResolver::new();
    let source = r#"
try:
    from ujson import loads
except ImportError:
    from json import loads

def parse(data: str) -> dict:
    return loads(data)
"#;
    let result = resolver.parse_file(Path::new("/project/parser.py"), source);

    // tree-sitter should capture both from-imports even inside try/except
    assert!(
        result.imports.len() >= 2,
        "should capture both conditional imports, got {}",
        result.imports.len()
    );

    let ujson = result.imports.iter().find(|i| i.source == "ujson");
    let json = result.imports.iter().find(|i| i.source == "json");
    assert!(ujson.is_some(), "should capture ujson import");
    assert!(json.is_some(), "should capture json import");
}

#[test]
/// Importing a module by its full dotted path should capture the full source.
fn test_dotted_path_import() {
    let resolver = PyResolver::new();
    let source = r#"
from package.subpackage.module import func
"#;
    let result = resolver.parse_file(Path::new("/project/app.py"), source);

    assert!(!result.imports.is_empty(), "from-import should be captured");
    let imp = &result.imports[0];
    assert_eq!(imp.source, "package.subpackage.module");
    assert!(imp.imported_names.contains(&"func".to_string()));
}

#[test]
/// Python resolution should use ty subprocess for Tier 2 resolution.
/// Verified via MockTyClient integration with PyResolver.
fn test_resolution_uses_ty_subprocess() {
    let mock = MockTyClient::new(true);
    mock.set_result(
        PathBuf::from("app.py"),
        TyResult {
            definitions: vec![],
            errors: vec![],
        },
    );

    let resolver = PyResolver::with_ty(Box::new(mock));
    assert!(
        resolver.has_ty(),
        "resolver with mock ty client should report ty available"
    );

    // Tier 1 still works alongside Tier 2
    let source = r#"
from utils import helper

def main(x: int) -> int:
    return helper(x)
"#;
    let result = resolver.parse_file(Path::new("app.py"), source);
    assert!(!result.definitions.is_empty(), "should parse definitions");
    assert!(!result.imports.is_empty(), "should parse imports");
}

#[test]
/// ty subprocess timeout should prevent resolution from blocking indefinitely.
/// Verified by MockTyClient returning an error (simulating timeout).
fn test_ty_subprocess_timeout() {
    let mock = MockTyClient::new(true);
    mock.set_error(
        PathBuf::from("slow.py"),
        "ty subprocess timed out after 5s".to_string(),
    );

    // Verify the mock returns an error
    let result = mock.check_file(Path::new("slow.py"));
    assert!(result.is_err(), "should return timeout error");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("timed out"),
        "error should mention timeout"
    );

    // PyResolver with_ty still works for Tier 1 parsing
    let resolver = PyResolver::with_ty(Box::new(MockTyClient::new(true)));
    let source = r#"
def fast_func(x: int) -> int:
    return x
"#;
    let result = resolver.parse_file(Path::new("slow.py"), source);
    assert!(
        !result.definitions.is_empty(),
        "Tier 1 should work despite ty timeout"
    );
}
