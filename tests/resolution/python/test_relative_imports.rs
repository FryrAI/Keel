// Tests for Python relative import resolution (Spec 003 - Python Resolution)
//
// use keel_parsers::python::TyResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Single-dot relative import should resolve to a sibling module.
fn test_single_dot_relative_import() {
    // GIVEN package/a.py with `from .b import process`
    // WHEN the import is resolved
    // THEN it resolves to the process function in package/b.py
}

#[test]
#[ignore = "Not yet implemented"]
/// Double-dot relative import should resolve to a parent package module.
fn test_double_dot_relative_import() {
    // GIVEN package/sub/a.py with `from ..utils import helper`
    // WHEN the import is resolved
    // THEN it resolves to the helper function in package/utils.py
}

#[test]
#[ignore = "Not yet implemented"]
/// Triple-dot relative import should resolve to a grandparent package.
fn test_triple_dot_relative_import() {
    // GIVEN package/sub/deep/a.py with `from ...core import engine`
    // WHEN the import is resolved
    // THEN it resolves to the engine function in package/core.py
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
#[ignore = "Not yet implemented"]
/// Relative import from __init__.py should resolve within the package.
fn test_relative_import_from_init() {
    // GIVEN package/__init__.py with `from .module import func`
    // WHEN the import is resolved
    // THEN it resolves to func in package/module.py
}

#[test]
#[ignore = "Not yet implemented"]
/// Relative import of a subpackage should resolve to the subpackage's __init__.py.
fn test_relative_import_subpackage() {
    // GIVEN package/a.py with `from .sub import handler`
    // WHEN the import is resolved
    // THEN it resolves through package/sub/__init__.py
}
