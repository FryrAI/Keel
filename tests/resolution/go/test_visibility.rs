// Tests for Go capitalization-based visibility (Spec 004 - Go Resolution)
//
// use keel_parsers::go::GoHeuristicResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Capitalized functions should be marked as exported (public) visibility.
fn test_capitalized_function_is_exported() {
    // GIVEN `func ProcessData()` in package handlers
    // WHEN the function's visibility is determined
    // THEN it is marked as exported/public
}

#[test]
#[ignore = "Not yet implemented"]
/// Lowercase functions should be marked as unexported (package-private) visibility.
fn test_lowercase_function_is_unexported() {
    // GIVEN `func helper()` in package handlers
    // WHEN the function's visibility is determined
    // THEN it is marked as unexported/package-private
}

#[test]
#[ignore = "Not yet implemented"]
/// Cross-package calls to unexported functions should produce resolution errors.
fn test_cross_package_unexported_call_error() {
    // GIVEN package A calling package B's unexported function `helper()`
    // WHEN the call is resolved
    // THEN a resolution error is produced (unexported function not accessible)
}

#[test]
#[ignore = "Not yet implemented"]
/// Struct fields follow the same capitalization visibility rules.
fn test_struct_field_visibility() {
    // GIVEN a struct with `Name string` (exported) and `age int` (unexported)
    // WHEN field visibility is determined
    // THEN Name is public and age is package-private
}

#[test]
#[ignore = "Not yet implemented"]
/// Capitalized types (struct, interface) should be exported.
fn test_capitalized_type_visibility() {
    // GIVEN `type UserService struct {}` and `type config struct {}`
    // WHEN type visibility is determined
    // THEN UserService is exported and config is unexported
}
