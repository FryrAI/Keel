// Tests for Go package-level scoping resolution (Spec 004 - Go Resolution)
//
// use keel_parsers::go::GoHeuristicResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Functions within the same package should resolve to each other without imports.
fn test_same_package_function_resolution() {
    // GIVEN two files in package "handlers": a.go defines Process(), b.go calls Process()
    // WHEN the call site in b.go is resolved
    // THEN it resolves to Process() in a.go within the same package
}

#[test]
#[ignore = "Not yet implemented"]
/// Functions in different packages require explicit import to resolve.
fn test_cross_package_requires_import() {
    // GIVEN package "handlers" calling package "utils".Process()
    // WHEN the call site is resolved
    // THEN it resolves via the import statement to utils.Process()
}

#[test]
#[ignore = "Not yet implemented"]
/// Package-level variables should be accessible from any file in the same package.
fn test_package_level_variable_resolution() {
    // GIVEN a package-level var defined in a.go and referenced in b.go
    // WHEN the reference is resolved
    // THEN it resolves to the variable definition in a.go
}

#[test]
#[ignore = "Not yet implemented"]
/// Multiple files in the same package should share the same scope.
fn test_multi_file_package_scope() {
    // GIVEN 5 files in package "models" all defining functions
    // WHEN cross-file function calls are resolved
    // THEN all resolve correctly within the package scope
}

#[test]
#[ignore = "Not yet implemented"]
/// Test packages (_test.go) should be treated as the same package scope.
fn test_test_file_package_scope() {
    // GIVEN handler.go and handler_test.go in the same package
    // WHEN the test file calls functions from handler.go
    // THEN they resolve correctly (same package scope)
}
