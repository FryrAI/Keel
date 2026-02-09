// Tests for Go import path resolution (Spec 004 - Go Resolution)
//
// use keel_parsers::go::GoHeuristicResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Standard library imports should be recognized as external.
fn test_stdlib_import_resolution() {
    // GIVEN `import "fmt"` in a Go file
    // WHEN the import is resolved
    // THEN it is recognized as a standard library import
}

#[test]
#[ignore = "Not yet implemented"]
/// Module-relative imports should resolve to the correct local package.
fn test_module_relative_import() {
    // GIVEN go.mod declaring module "github.com/user/project"
    // WHEN `import "github.com/user/project/pkg/utils"` is resolved
    // THEN it resolves to the local pkg/utils/ directory
}

#[test]
#[ignore = "Not yet implemented"]
/// Aliased imports should track the alias for call resolution.
fn test_aliased_import() {
    // GIVEN `import u "github.com/user/project/utils"`
    // WHEN `u.Process()` is called and resolved
    // THEN it resolves to Process() in the utils package via the alias
}

#[test]
#[ignore = "Not yet implemented"]
/// Dot imports should import all exported names into the current scope.
fn test_dot_import() {
    // GIVEN `import . "github.com/user/project/utils"`
    // WHEN `Process()` is called (without package prefix)
    // THEN it resolves to utils.Process() via the dot import
}

#[test]
#[ignore = "Not yet implemented"]
/// Blank imports (side-effect only) should be tracked but not produce call edges.
fn test_blank_import() {
    // GIVEN `import _ "github.com/lib/pq"`
    // WHEN the import is analyzed
    // THEN it is tracked as a dependency but no symbol resolution is attempted
}
