// Tests for Python star import resolution (Spec 003 - Python Resolution)

use std::path::Path;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};

#[test]
#[ignore = "BUG: star import resolution requires __all__ parsing not implemented in parser"]
/// Star import should import all public names from the target module.
fn test_star_import_all_public_names() {
    // Requires multi-file resolution with __all__ introspection
    // which is a Tier 2 feature via ty subprocess.
}

#[test]
#[ignore = "BUG: __all__ parsing for star imports not implemented in parser layer"]
/// Star import with __all__ defined should only import names listed in __all__.
fn test_star_import_respects_all() {
    // Requires parsing __all__ from the target module, a Tier 2 feature.
}

#[test]
#[ignore = "BUG: star import confidence scoring not implemented"]
/// Star import should produce lower confidence edges due to ambiguity.
fn test_star_import_lower_confidence() {
    // Confidence scoring for star imports requires knowledge of what
    // names are available via the star import, a Tier 2 feature.
}

#[test]
/// Explicit import should produce higher confidence call edge (0.80).
fn test_explicit_import_confidence() {
    let resolver = PyResolver::new();

    // Parse utils module with a function
    let utils_source = r#"
def foo():
    return 42
"#;
    resolver.parse_file(Path::new("/project/utils.py"), utils_source);

    // Parse caller with explicit import
    let caller_source = r#"
from utils import foo

def main():
    foo()
"#;
    let caller_path = Path::new("/project/app.py");
    resolver.parse_file(caller_path, caller_source);

    // Resolve the call edge
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 5,
        callee_name: "foo".into(),
        receiver: None,
    });

    assert!(edge.is_some(), "Expected call edge to be resolved via explicit import");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "foo");
    assert_eq!(edge.target_file, "utils");
    assert!(
        edge.confidence >= 0.80,
        "Explicit import edge should have confidence >= 0.80, got: {}",
        edge.confidence
    );
}

#[test]
/// Same-file call edge should produce highest confidence (0.95).
fn test_same_file_call_edge_confidence() {
    let resolver = PyResolver::new();

    let source = r#"
def helper():
    return 1

def main():
    helper()
"#;
    let path = Path::new("/project/app.py");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 6,
        callee_name: "helper".into(),
        receiver: None,
    });

    assert!(edge.is_some(), "Expected same-file call edge to be resolved");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert_eq!(edge.target_file, "/project/app.py");
    assert!(
        edge.confidence >= 0.95,
        "Same-file call edge should have confidence >= 0.95, got: {}",
        edge.confidence
    );
}

#[test]
/// Unresolvable call should return None.
fn test_unresolvable_call_returns_none() {
    let resolver = PyResolver::new();

    let source = r#"
def main():
    unknown_function()
"#;
    let path = Path::new("/project/app.py");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 3,
        callee_name: "unknown_function".into(),
        receiver: None,
    });

    assert!(edge.is_none(), "Unresolvable call should return None");
}

#[test]
#[ignore = "BUG: multiple star import disambiguation not implemented"]
/// Multiple star imports should track all potential sources for ambiguous names.
fn test_multiple_star_imports_ambiguity() {
    // Requires multi-file star import resolution with ambiguity tracking.
}

#[test]
#[ignore = "BUG: package __init__.py star import not implemented"]
/// Star import from a package should use the package's __init__.py exports.
fn test_star_import_from_package() {
    // Requires __init__.py resolution, a Tier 2 feature.
}

#[test]
#[ignore = "BUG: star import chain tracing not implemented"]
/// Star import chains (a imports * from b, b imports * from c) should be traced.
fn test_star_import_chain() {
    // Requires multi-file chain resolution, a Tier 2 feature.
}
