// Tests for Python relative import resolution (Spec 003 - Python Resolution)

use std::path::Path;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;

#[test]
/// Single-dot relative import should resolve to a sibling module.
fn test_single_dot_relative_import() {
    // GIVEN package/a.py with `from .b import process`
    let resolver = PyResolver::new();
    let source = r#"
from .b import process

def main():
    process()
"#;
    let path = Path::new("/project/package/a.py");
    let result = resolver.parse_file(path, source);

    // THEN the import is marked as relative and source resolves to sibling
    assert_eq!(result.imports.len(), 1);
    let imp = &result.imports[0];
    assert!(imp.is_relative);
    assert!(imp.source.contains("b.py"), "Expected source to contain b.py, got: {}", imp.source);
    assert!(imp.imported_names.contains(&"process".to_string()));
}

#[test]
/// Double-dot relative import should resolve to a parent package module.
fn test_double_dot_relative_import() {
    // GIVEN package/sub/a.py with `from ..utils import helper`
    let resolver = PyResolver::new();
    let source = r#"
from ..utils import helper
"#;
    let path = Path::new("/project/package/sub/a.py");
    let result = resolver.parse_file(path, source);

    // THEN the import resolves to the parent package's utils module
    assert_eq!(result.imports.len(), 1);
    let imp = &result.imports[0];
    assert!(imp.is_relative);
    // Double dot from /project/package/sub/ should go up to /project/package/utils.py
    assert!(
        imp.source.contains("package") && imp.source.contains("utils"),
        "Expected source to resolve to package/utils, got: {}",
        imp.source
    );
    assert!(imp.imported_names.contains(&"helper".to_string()));
}

#[test]
/// Triple-dot relative import should resolve to a grandparent package.
fn test_triple_dot_relative_import() {
    // GIVEN package/sub/deep/a.py with `from ...core import engine`
    let resolver = PyResolver::new();
    let source = r#"
from ...core import engine
"#;
    let path = Path::new("/project/package/sub/deep/a.py");
    let result = resolver.parse_file(path, source);

    // THEN the import resolves up two levels from parent dir
    assert_eq!(result.imports.len(), 1);
    let imp = &result.imports[0];
    assert!(imp.is_relative);
    // Triple dot from /project/package/sub/deep/ -> up to /project/package/core.py
    assert!(
        imp.source.contains("core"),
        "Expected source to contain core, got: {}",
        imp.source
    );
}

#[test]
#[ignore = "Not yet implemented"]
/// Relative import going beyond the top-level package should produce an error.
fn test_relative_import_beyond_package_root() {
    // GIVEN a top-level package with a relative import that exceeds package depth
    // WHEN the import is resolved
    // THEN a resolution error is produced
}

#[test]
/// Relative import from __init__.py should resolve within the package.
fn test_relative_import_from_init() {
    // GIVEN package/__init__.py with `from .module import func`
    let resolver = PyResolver::new();
    let source = r#"
from .module import func
"#;
    let path = Path::new("/project/package/__init__.py");
    let result = resolver.parse_file(path, source);

    // THEN it resolves to func in package/module.py
    assert_eq!(result.imports.len(), 1);
    let imp = &result.imports[0];
    assert!(imp.is_relative);
    assert!(
        imp.source.contains("module"),
        "Expected source to contain module, got: {}",
        imp.source
    );
    assert!(imp.imported_names.contains(&"func".to_string()));
}

#[test]
/// Relative import of a subpackage should resolve to the subpackage path.
fn test_relative_import_subpackage() {
    // GIVEN package/a.py with `from .sub import handler`
    let resolver = PyResolver::new();
    let source = r#"
from .sub import handler
"#;
    let path = Path::new("/project/package/a.py");
    let result = resolver.parse_file(path, source);

    // THEN it resolves through package/sub
    assert_eq!(result.imports.len(), 1);
    let imp = &result.imports[0];
    assert!(imp.is_relative);
    assert!(
        imp.source.contains("sub"),
        "Expected source to contain sub, got: {}",
        imp.source
    );
    assert!(imp.imported_names.contains(&"handler".to_string()));
}
