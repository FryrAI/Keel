// Tests for progressive adoption (new vs existing code) (Spec 006 - Enforcement Engine)
//
// use keel_enforce::adoption::ProgressiveAdoption;

#[test]
#[ignore = "Not yet implemented"]
/// New code (added after keel init) should produce ERROR on violations.
fn test_new_code_produces_error() {
    // GIVEN a function added after keel init with missing type hints
    // WHEN enforcement runs
    // THEN E002 is produced as ERROR
}

#[test]
#[ignore = "Not yet implemented"]
/// Pre-existing code should produce WARNING on violations (not ERROR).
fn test_existing_code_produces_warning() {
    // GIVEN a function that existed before keel init with missing type hints
    // WHEN enforcement runs
    // THEN E002 is produced as WARNING (not ERROR)
}

#[test]
#[ignore = "Not yet implemented"]
/// Modified pre-existing code should escalate to ERROR (touched = new rules apply).
fn test_modified_existing_code_escalates_to_error() {
    // GIVEN a pre-existing function that has been modified
    // WHEN enforcement runs
    // THEN violations are ERROR (modification triggers new-code rules)
}

#[test]
#[ignore = "Not yet implemented"]
/// Progressive adoption should track file modification timestamps.
fn test_tracks_modification_timestamps() {
    // GIVEN a project with keel init timestamp and file modification times
    // WHEN new-vs-existing classification runs
    // THEN files modified after init are classified as "new code"
}

#[test]
#[ignore = "Not yet implemented"]
/// Configurable escalation should allow promoting all WARNINGs to ERRORs.
fn test_configurable_escalation() {
    // GIVEN keel.toml with `escalate_existing = true`
    // WHEN enforcement runs on pre-existing code
    // THEN all violations are ERROR (not WARNING)
}

#[test]
#[ignore = "Not yet implemented"]
/// New functions in pre-existing files should be treated as new code.
fn test_new_function_in_existing_file() {
    // GIVEN a new function added to a pre-existing file
    // WHEN enforcement runs
    // THEN the new function's violations are ERROR (new code rules)
}

#[test]
#[ignore = "Not yet implemented"]
/// Pre-existing code with no modifications should remain at WARNING level.
fn test_untouched_existing_code_stays_warning() {
    // GIVEN a pre-existing function with no modifications since keel init
    // WHEN enforcement runs repeatedly
    // THEN violations remain at WARNING level
}

#[test]
#[ignore = "Not yet implemented"]
/// keel init should record the baseline timestamp for progressive adoption.
fn test_init_records_baseline() {
    // GIVEN a fresh project
    // WHEN `keel init` is run
    // THEN a baseline timestamp is recorded for progressive adoption classification
}
