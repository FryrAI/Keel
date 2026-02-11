// Tests for .keelignore file handling (Spec 001 - Tree-sitter Foundation)
//
// use keel_parsers::walker::FileWalker;  // keelignore handled via walker

#[test]
#[ignore = "Not yet implemented"]
/// Files matching .keelignore patterns should be excluded from parsing.
fn test_keelignore_excludes_matching_files() {
    // GIVEN a .keelignore file with pattern "*.test.ts"
    // WHEN the parser scans for source files
    // THEN files ending in .test.ts are excluded
}

#[test]
#[ignore = "Not yet implemented"]
/// Directory patterns in .keelignore should exclude entire directories.
fn test_keelignore_excludes_directories() {
    // GIVEN a .keelignore file with pattern "node_modules/"
    // WHEN the parser scans for source files
    // THEN all files under node_modules/ are excluded
}

#[test]
#[ignore = "Not yet implemented"]
/// Glob patterns in .keelignore should work with wildcards.
fn test_keelignore_glob_patterns() {
    // GIVEN a .keelignore file with pattern "src/**/generated/*.ts"
    // WHEN the parser scans for source files
    // THEN matching files in any nested generated/ directory are excluded
}

#[test]
#[ignore = "Not yet implemented"]
/// Negation patterns (!) should re-include previously excluded files.
fn test_keelignore_negation_pattern() {
    // GIVEN a .keelignore with "*.test.ts" and "!critical.test.ts"
    // WHEN the parser scans for source files
    // THEN critical.test.ts is included despite the *.test.ts exclusion
}

#[test]
#[ignore = "Not yet implemented"]
/// Missing .keelignore file should result in no exclusions (all files parsed).
fn test_keelignore_missing_file() {
    // GIVEN a project with no .keelignore file
    // WHEN the parser scans for source files
    // THEN all recognized source files are included
}

#[test]
#[ignore = "Not yet implemented"]
/// Default ignores (node_modules, .git, vendor, target) should apply even without .keelignore.
fn test_keelignore_default_ignores() {
    // GIVEN a project with no .keelignore but with node_modules/ and .git/ directories
    // WHEN the parser scans for source files
    // THEN node_modules/ and .git/ are excluded by default
}

#[test]
#[ignore = "Not yet implemented"]
/// Comments in .keelignore (lines starting with #) should be ignored.
fn test_keelignore_comments_ignored() {
    // GIVEN a .keelignore file with comment lines starting with #
    // WHEN the ignore rules are parsed
    // THEN comment lines do not affect file inclusion/exclusion
}
