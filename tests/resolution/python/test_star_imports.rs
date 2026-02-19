// Tests for Python star import resolution (Spec 003 - Python Resolution)

use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};
use std::path::Path;

#[test]
/// Star import should import all public names from the target module.
fn test_star_import_all_public_names() {
    let resolver = PyResolver::new();

    // Parse target module with public functions
    let utils_source = r#"
def foo():
    return 42

def bar():
    return 99
"#;
    resolver.parse_file(Path::new("/project/utils.py"), utils_source);

    // Parse caller with star import
    let caller_source = r#"
from utils import *

def main():
    foo()
"#;
    resolver.parse_file(Path::new("/project/app.py"), caller_source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 5,
        callee_name: "foo".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "Star import should resolve public name 'foo' from target module"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "foo");
    assert_eq!(edge.resolution_tier, "tier2_heuristic");
}

#[test]
/// Star import with __all__ defined should resolve names in __all__ at 0.65.
fn test_star_import_respects_all() {
    let resolver = PyResolver::new();

    // Parse target module with __all__ restricting exports
    let utils_source = r#"
__all__ = ['foo']

def foo():
    return 42

def bar():
    return 99
"#;
    resolver.parse_file(Path::new("/project/utils.py"), utils_source);

    // Parse caller with star import
    let caller_source = r#"
from utils import *

def main():
    foo()
"#;
    resolver.parse_file(Path::new("/project/app.py"), caller_source);

    // foo is in __all__ — should resolve with higher confidence
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 5,
        callee_name: "foo".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "foo should resolve via star import (__all__)"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "foo");
    assert!(
        edge.confidence >= 0.60,
        "Name in __all__ via star import should have confidence >= 0.60, got: {}",
        edge.confidence
    );
}

#[test]
/// Star import should produce lower confidence edges than explicit import.
fn test_star_import_lower_confidence() {
    let resolver = PyResolver::new();

    // Parse target module
    let utils_source = r#"
def process():
    return "done"
"#;
    resolver.parse_file(Path::new("/project/utils.py"), utils_source);

    // Parse caller with star import
    let star_caller = r#"
from utils import *

def main():
    process()
"#;
    resolver.parse_file(Path::new("/project/star_app.py"), star_caller);

    // Parse another caller with explicit import
    let explicit_caller = r#"
from utils import process

def main():
    process()
"#;
    resolver.parse_file(Path::new("/project/explicit_app.py"), explicit_caller);

    // Resolve via star import
    let star_edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/star_app.py".into(),
        line: 5,
        callee_name: "process".into(),
        receiver: None,
    });

    // Resolve via explicit import
    let explicit_edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/explicit_app.py".into(),
        line: 5,
        callee_name: "process".into(),
        receiver: None,
    });

    assert!(star_edge.is_some(), "Star import should resolve");
    assert!(explicit_edge.is_some(), "Explicit import should resolve");

    let star_conf = star_edge.unwrap().confidence;
    let explicit_conf = explicit_edge.unwrap().confidence;

    assert!(
        star_conf < explicit_conf,
        "Star import confidence ({}) should be less than explicit import confidence ({})",
        star_conf,
        explicit_conf
    );
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

    assert!(
        edge.is_some(),
        "Expected call edge to be resolved via explicit import"
    );
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

    assert!(
        edge.is_some(),
        "Expected same-file call edge to be resolved"
    );
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
/// Multiple star imports with same name in both should produce low confidence.
fn test_multiple_star_imports_ambiguity() {
    let resolver = PyResolver::new();

    // Parse two target modules, each with a 'process' function
    let mod_a_source = r#"
def process():
    return "from A"
"#;
    resolver.parse_file(Path::new("/project/mod_a.py"), mod_a_source);

    let mod_b_source = r#"
def process():
    return "from B"
"#;
    resolver.parse_file(Path::new("/project/mod_b.py"), mod_b_source);

    // Parse caller with two star imports
    let caller_source = r#"
from mod_a import *
from mod_b import *

def main():
    process()
"#;
    resolver.parse_file(Path::new("/project/app.py"), caller_source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 6,
        callee_name: "process".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Ambiguous star import should still resolve");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "process");
    assert!(
        edge.confidence <= 0.50,
        "Ambiguous star import should have low confidence (<= 0.50), got: {}",
        edge.confidence
    );
}

#[test]
/// Star import from a package should resolve via cache lookup.
fn test_star_import_from_package() {
    let resolver = PyResolver::new();

    // Parse target package __init__.py
    let init_source = r#"
def exported_func():
    return "from package"
"#;
    resolver.parse_file(Path::new("/project/mypkg/__init__.py"), init_source);

    // Parse caller with star import from the package
    let caller_source = r#"
from mypkg import *

def main():
    exported_func()
"#;
    resolver.parse_file(Path::new("/project/app.py"), caller_source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/app.py".into(),
        line: 5,
        callee_name: "exported_func".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "Star import from package should resolve via __init__.py cache"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "exported_func");
}

#[test]
/// Star import chain (a imports * from b, b imports * from c) should have
/// low confidence since we don't chase transitive star imports.
fn test_star_import_chain() {
    let resolver = PyResolver::new();

    // Module C defines the actual function
    let mod_c_source = r#"
def deep_func():
    return "from C"
"#;
    resolver.parse_file(Path::new("/project/mod_c.py"), mod_c_source);

    // Module B re-exports via star import from C, and defines its own func
    let mod_b_source = r#"
from mod_c import *

def deep_func():
    return "from B (shadows C)"
"#;
    resolver.parse_file(Path::new("/project/mod_b.py"), mod_b_source);

    // Module A imports * from B
    let mod_a_source = r#"
from mod_b import *

def main():
    deep_func()
"#;
    resolver.parse_file(Path::new("/project/mod_a.py"), mod_a_source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/mod_a.py".into(),
        line: 5,
        callee_name: "deep_func".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "Star import chain should still resolve (with low confidence)"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "deep_func");
    assert!(
        edge.confidence <= 0.50,
        "Star import chain should have low confidence (<= 0.50), got: {}",
        edge.confidence
    );
}
