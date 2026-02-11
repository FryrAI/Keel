// Tests for W002 duplicate function name detection (Spec 006 - Enforcement Engine)
//
// use keel_enforce::violations_extended::check_duplicate_names;

#[test]
#[ignore = "Not yet implemented"]
/// Two functions with the same name in different modules should produce W002.
fn test_w002_duplicate_name_across_modules() {
    // GIVEN process() defined in both module_a.py and module_b.py
    // WHEN enforcement runs
    // THEN W002 is produced flagging the duplicate name
}

#[test]
#[ignore = "Not yet implemented"]
/// Functions with the same name but different signatures should still produce W002.
fn test_w002_same_name_different_signatures() {
    // GIVEN process(data: str) in module_a and process(items: list) in module_b
    // WHEN enforcement runs
    // THEN W002 is produced (same name, different signatures)
}

#[test]
#[ignore = "Not yet implemented"]
/// Methods with the same name in different classes should NOT produce W002.
fn test_w002_same_method_different_classes_no_warning() {
    // GIVEN class A with method process() and class B with method process()
    // WHEN enforcement runs
    // THEN no W002 is produced (methods are scoped to their class)
}

#[test]
#[ignore = "Not yet implemented"]
/// W002 severity should always be WARNING.
fn test_w002_severity_is_warning() {
    // GIVEN a duplicate function name scenario
    // WHEN W002 is produced
    // THEN the severity is WARNING
}

#[test]
#[ignore = "Not yet implemented"]
/// W002 should include the file paths of all duplicate definitions.
fn test_w002_includes_all_locations() {
    // GIVEN process() defined in 3 different modules
    // WHEN W002 is produced
    // THEN all 3 file paths and line numbers are included
}

#[test]
#[ignore = "Not yet implemented"]
/// Common utility function names (init, setup, teardown) should have relaxed W002 scoring.
fn test_w002_relaxed_for_common_names() {
    // GIVEN init() defined in multiple modules
    // WHEN enforcement runs
    // THEN W002 is produced with lower priority for common utility names
}

#[test]
#[ignore = "Not yet implemented"]
/// Functions in test files should not trigger W002 against non-test files.
fn test_w002_test_files_excluded() {
    // GIVEN process() in main code and process() in test_module.py
    // WHEN enforcement runs
    // THEN no W002 is produced (test files are excluded from duplicate detection)
}

#[test]
#[ignore = "Not yet implemented"]
/// Duplicate detection should be case-sensitive.
fn test_w002_case_sensitive() {
    // GIVEN Process() and process() in different modules
    // WHEN enforcement runs
    // THEN no W002 is produced (different casing = different names)
}
