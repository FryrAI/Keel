// Tests for Python package and module resolution (Spec 003 - Python Resolution)
//
// Most package resolution features require filesystem access and the ty
// subprocess (Tier 2), which are not available at the parser layer.

use std::path::Path;
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
#[ignore = "BUG: ty subprocess integration not testable without ty binary"]
/// Python resolution should use ty subprocess for Tier 2 resolution.
fn test_resolution_uses_ty_subprocess() {
    // ty subprocess is invoked externally and requires the ty binary to be
    // installed. This test requires integration test infrastructure with
    // ty available on PATH.
}

#[test]
#[ignore = "BUG: ty subprocess timeout not testable in unit tests"]
/// ty subprocess timeout should prevent resolution from blocking indefinitely.
fn test_ty_subprocess_timeout() {
    // Timeout handling requires spawning a real subprocess, which is
    // an integration test concern.
}
