// Tests for file watch mode in `keel serve` (Spec 010)
//
// Validates that the server detects file changes, triggers incremental
// compilation, debounces rapid changes, and notifies connected clients.
//
// use keel_server::watch::{FileWatcher, WatchConfig, WatchEvent};
// use std::fs;
// use std::time::Duration;
// use tempfile::TempDir;

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_detects_file_modification() {
    // GIVEN a keel server in watch mode monitoring a project directory
    // WHEN a tracked source file is modified on disk
    // THEN the watcher emits a change event for that file within the debounce window
}

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_triggers_incremental_compile_on_change() {
    // GIVEN a keel server in watch mode with a clean compile state
    // WHEN a source file is modified to introduce a broken caller (E001)
    // THEN an incremental compile is triggered and the violation is reported
}

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_debounces_rapid_changes() {
    // GIVEN a keel server in watch mode with a debounce window of 300ms
    // WHEN the same file is modified 10 times within 100ms
    // THEN only a single compile is triggered after the debounce period
}

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_ignores_non_source_files() {
    // GIVEN a keel server in watch mode monitoring a project directory
    // WHEN a non-source file (e.g., .gitignore, README.md, image.png) is modified
    // THEN no compile is triggered and the watcher produces no events
}

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_handles_file_creation() {
    // GIVEN a keel server in watch mode monitoring a project directory
    // WHEN a new source file is created in a tracked directory
    // THEN the watcher detects the new file and triggers a map + compile
}

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_handles_file_deletion() {
    // GIVEN a keel server in watch mode with a mapped project containing file A
    // WHEN file A is deleted from disk
    // THEN the watcher detects the deletion and triggers graph cleanup for file A's nodes
}

#[test]
#[ignore = "Not yet implemented"]
fn test_watch_respects_gitignore_patterns() {
    // GIVEN a keel server in watch mode with a .gitignore that excludes node_modules/
    // WHEN a file inside node_modules/ is modified
    // THEN no compile is triggered and the watcher ignores the change
}
