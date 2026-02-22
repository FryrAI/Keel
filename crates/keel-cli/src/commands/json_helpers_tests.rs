use super::*;

// --- extract_json_string ---

#[test]
fn string_basic() {
    let json = r#"{"name":"alice","role":"admin"}"#;
    assert_eq!(extract_json_string(json, "name"), Some("alice".into()));
    assert_eq!(extract_json_string(json, "role"), Some("admin".into()));
}

#[test]
fn string_with_whitespace() {
    let json = r#"{ "token" : "abc-123" }"#;
    assert_eq!(extract_json_string(json, "token"), Some("abc-123".into()));
}

#[test]
fn string_missing_key() {
    let json = r#"{"name":"alice"}"#;
    assert_eq!(extract_json_string(json, "missing"), None);
}

#[test]
fn string_empty_value() {
    let json = r#"{"key":""}"#;
    assert_eq!(extract_json_string(json, "key"), Some("".into()));
}

#[test]
fn string_empty_json() {
    assert_eq!(extract_json_string("", "key"), None);
    assert_eq!(extract_json_string("{}", "key"), None);
}

#[test]
fn string_number_value_returns_none() {
    // Value is a number, not a string — no opening quote after colon
    let json = r#"{"count":42}"#;
    assert_eq!(extract_json_string(json, "count"), None);
}

// --- extract_json_number ---

#[test]
fn number_basic() {
    let json = r#"{"interval":5,"expires_in":900}"#;
    assert_eq!(extract_json_number(json, "interval"), Some(5));
    assert_eq!(extract_json_number(json, "expires_in"), Some(900));
}

#[test]
fn number_with_whitespace() {
    let json = r#"{ "count" : 42 }"#;
    assert_eq!(extract_json_number(json, "count"), Some(42));
}

#[test]
fn number_missing_key() {
    let json = r#"{"count":42}"#;
    assert_eq!(extract_json_number(json, "missing"), None);
}

#[test]
fn number_string_value_returns_none() {
    // Value is a string, not a number — starts with quote, no digits
    let json = r#"{"name":"alice"}"#;
    assert_eq!(extract_json_number(json, "name"), None);
}

#[test]
fn number_zero() {
    let json = r#"{"val":0}"#;
    assert_eq!(extract_json_number(json, "val"), Some(0));
}

#[test]
fn number_large_value() {
    let json = r#"{"ts":1700000000}"#;
    assert_eq!(extract_json_number(json, "ts"), Some(1700000000));
}
