use super::*;

#[test]
fn test_extract_keywords() {
    let kw = extract_keywords("validate user JWT token and check expiry");
    assert!(kw.contains(&"validate".to_string()));
    assert!(kw.contains(&"jwt".to_string()));
    assert!(!kw.contains(&"and".to_string())); // stop word
}

#[test]
fn test_keyword_score() {
    let desc = vec!["auth".to_string(), "jwt".to_string(), "validate".to_string()];
    let module_kw = vec!["auth".to_string(), "jwt".to_string(), "session".to_string()];
    let score = compute_keyword_score(&desc, &module_kw);
    assert!(score > 0.5); // 2/3 match
}

#[test]
fn test_keyword_score_no_overlap() {
    let desc = vec!["database".to_string(), "query".to_string()];
    let module_kw = vec!["auth".to_string(), "jwt".to_string()];
    let score = compute_keyword_score(&desc, &module_kw);
    assert_eq!(score, 0.0);
}

#[test]
fn test_detect_snake_case() {
    let names = vec!["validate_token", "validate_session", "check_auth"];
    let conv = detect_convention(&names);
    assert!(matches!(conv, NamingConvention::SnakeCase { .. }));
    assert!(conv.to_string().contains("snake_case"));
    assert!(conv.to_string().contains("prefix: validate_"));
}

#[test]
fn test_detect_camel_case() {
    let names = vec!["validateToken", "validateSession", "checkAuth"];
    let conv = detect_convention(&names);
    assert!(matches!(conv, NamingConvention::CamelCase { .. }));
}

#[test]
fn test_generate_snake_name() {
    let words = vec!["validate".to_string(), "jwt".to_string(), "expiry".to_string()];
    let conv = NamingConvention::SnakeCase {
        prefix: Some("check_".to_string()),
    };
    let name = generate_name(&words, &conv);
    assert_eq!(name, "check_validate_jwt_expiry");
}

#[test]
fn test_generate_camel_name() {
    let words = vec!["validate".to_string(), "jwt".to_string(), "expiry".to_string()];
    let conv = NamingConvention::CamelCase { prefix: None };
    let name = generate_name(&words, &conv);
    assert_eq!(name, "validateJwtExpiry");
}

#[test]
fn test_common_prefix_detection() {
    let names = vec!["get_user", "get_session", "get_token", "check_auth"];
    let prefix = detect_common_prefix(&names);
    assert_eq!(prefix, Some("get_".to_string()));
}

#[test]
fn test_no_common_prefix() {
    let names = vec!["create_user", "get_session", "delete_token", "check_auth"];
    let prefix = detect_common_prefix(&names);
    assert!(prefix.is_none());
}
