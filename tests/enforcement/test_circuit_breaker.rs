// Tests for circuit breaker behavior (Spec 006 - Enforcement Engine)
use keel_enforce::circuit_breaker::{BreakerAction, CircuitBreaker};

#[test]
fn test_circuit_breaker_attempt_1_fix_hint() {
    let mut cb = CircuitBreaker::new();
    let action = cb.record_failure("E001", "abc12345678", "src/lib.py");
    assert_eq!(action, BreakerAction::FixHint);
    assert_eq!(cb.failure_count("E001", "abc12345678", "src/lib.py"), 1);
}

#[test]
fn test_circuit_breaker_attempt_2_wider_discover() {
    let mut cb = CircuitBreaker::new();
    cb.record_failure("E001", "abc12345678", "src/lib.py");
    let action = cb.record_failure("E001", "abc12345678", "src/lib.py");
    assert_eq!(action, BreakerAction::WiderContext);
    assert_eq!(cb.failure_count("E001", "abc12345678", "src/lib.py"), 2);
}

#[test]
fn test_circuit_breaker_attempt_3_auto_downgrade() {
    let mut cb = CircuitBreaker::new();
    cb.record_failure("E001", "abc12345678", "src/lib.py");
    cb.record_failure("E001", "abc12345678", "src/lib.py");
    let action = cb.record_failure("E001", "abc12345678", "src/lib.py");
    assert_eq!(action, BreakerAction::Downgrade);
    assert!(cb.is_downgraded("E001", "abc12345678", "src/lib.py"));
}

#[test]
fn test_circuit_breaker_reset_on_success() {
    let mut cb = CircuitBreaker::new();
    cb.record_failure("E001", "abc", "file.rs");
    cb.record_failure("E001", "abc", "file.rs");
    assert_eq!(cb.failure_count("E001", "abc", "file.rs"), 2);

    cb.record_success("E001", "abc", "file.rs");
    assert_eq!(cb.failure_count("E001", "abc", "file.rs"), 0);
    assert!(!cb.is_downgraded("E001", "abc", "file.rs"));

    // Next failure starts from 1 again
    let action = cb.record_failure("E001", "abc", "file.rs");
    assert_eq!(action, BreakerAction::FixHint);
}

#[test]
fn test_circuit_breaker_independent_per_error_code() {
    let mut cb = CircuitBreaker::new();
    cb.record_failure("E001", "abc", "file.rs");
    cb.record_failure("E001", "abc", "file.rs");
    cb.record_failure("E002", "abc", "file.rs");

    assert_eq!(cb.failure_count("E001", "abc", "file.rs"), 2);
    assert_eq!(cb.failure_count("E002", "abc", "file.rs"), 1);
}

#[test]
fn test_circuit_breaker_independent_per_hash() {
    let mut cb = CircuitBreaker::new();
    // Hash A: 3 failures → downgraded
    cb.record_failure("E001", "hashA", "file.rs");
    cb.record_failure("E001", "hashA", "file.rs");
    cb.record_failure("E001", "hashA", "file.rs");

    // Hash B: 1 failure
    cb.record_failure("E001", "hashB", "file.rs");

    assert!(cb.is_downgraded("E001", "hashA", "file.rs"));
    assert!(!cb.is_downgraded("E001", "hashB", "file.rs"));
    assert_eq!(cb.failure_count("E001", "hashB", "file.rs"), 1);
}

#[test]
fn test_circuit_breaker_state_persistence() {
    let mut cb = CircuitBreaker::new();
    cb.record_failure("E001", "hash_abc", "src/a.rs");
    cb.record_failure("E001", "hash_abc", "src/a.rs");

    let state = cb.export_state();
    assert!(!state.is_empty());

    let mut cb2 = CircuitBreaker::new();
    cb2.import_state(&state);

    assert_eq!(cb2.failure_count("E001", "hash_abc", "src/a.rs"), 2);

    // Third failure should now downgrade
    let action = cb2.record_failure("E001", "hash_abc", "src/a.rs");
    assert_eq!(action, BreakerAction::Downgrade);
}

#[test]
fn test_circuit_breaker_empty_hash_uses_file_path() {
    let mut cb = CircuitBreaker::new();
    // E003 has empty hash, so file_path is used as identifier
    cb.record_failure("E003", "", "src/foo.py");
    cb.record_failure("E003", "", "src/foo.py");
    cb.record_failure("E003", "", "src/bar.py");

    // Different files have different counters
    assert_eq!(cb.failure_count("E003", "", "src/foo.py"), 2);
    assert_eq!(cb.failure_count("E003", "", "src/bar.py"), 1);
}

#[test]
fn test_circuit_breaker_custom_max_failures() {
    // With max_failures=2: attempt 1 → WiderContext (1 == 2-1), attempt 2 → Downgrade
    let mut cb = CircuitBreaker::with_max_failures(2);
    let a1 = cb.record_failure("E001", "h", "f.rs");
    assert_eq!(a1, BreakerAction::WiderContext);
    let a2 = cb.record_failure("E001", "h", "f.rs");
    assert_eq!(a2, BreakerAction::Downgrade);
    assert!(cb.is_downgraded("E001", "h", "f.rs"));
}

#[test]
fn test_circuit_breaker_sqlite_roundtrip() {
    let store = crate::common::in_memory_store();

    let mut cb = CircuitBreaker::new();
    cb.record_failure("E001", "h1", "a.rs");
    cb.record_failure("E001", "h1", "a.rs");
    cb.record_failure("E005", "h2", "b.rs");
    cb.record_failure("E005", "h2", "b.rs");
    cb.record_failure("E005", "h2", "b.rs");

    let state = cb.export_state();
    store.save_circuit_breaker(&state).unwrap();

    let loaded = store.load_circuit_breaker().unwrap();
    let mut cb2 = CircuitBreaker::new();
    cb2.import_state(&loaded);

    assert_eq!(cb2.failure_count("E001", "h1", "a.rs"), 2);
    assert!(cb2.is_downgraded("E005", "h2", "b.rs"));
}
