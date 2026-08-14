use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::GraphNode;

fn stored_node(id: u64, name: &str, kind: NodeKind, file: &str, module_id: u64) -> GraphNode {
    GraphNode {
        id,
        hash: format!("hash{id:07}"),
        kind,
        name: name.into(),
        signature: format!("fn {name}(value: &str) -> i64"),
        file_path: file.into(),
        line_start: id as u32,
        line_end: id as u32 + 4,
        docstring: Some("Parse a timestamp into Unix seconds.".into()),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        is_associated: false,
        complexity: 1,
        is_trivial_wrapper: false,
        in_test_context: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

#[test]
fn test_extract_keywords() {
    let kw = extract_keywords("validate user JWT token and check expiry");
    assert!(kw.contains(&"validate".to_string()));
    assert!(kw.contains(&"jwt".to_string()));
    assert!(!kw.contains(&"and".to_string())); // stop word
}

#[test]
fn test_keyword_score() {
    let desc = vec![
        "auth".to_string(),
        "jwt".to_string(),
        "validate".to_string(),
    ];
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
    let words = vec![
        "validate".to_string(),
        "jwt".to_string(),
        "expiry".to_string(),
    ];
    let conv = NamingConvention::SnakeCase {
        prefix: Some("check_".to_string()),
    };
    let name = generate_name(&words, &conv);
    assert_eq!(name, "check_validate_jwt_expiry");
}

#[test]
fn test_generate_camel_name() {
    let words = vec![
        "validate".to_string(),
        "jwt".to_string(),
        "expiry".to_string(),
    ];
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

#[test]
fn test_path_score_weights_higher_than_fn_name() {
    let desc = vec![
        "graph".to_string(),
        "data".to_string(),
        "export".to_string(),
    ];
    // graph_data.py matches 2/3 path segments
    let good_path = compute_path_score(&desc, "src/graph_data.py");
    // init_admin.py matches 0/3 path segments
    let bad_path = compute_path_score(&desc, "src/init_admin.py");
    assert!(
        good_path > bad_path,
        "graph_data.py should score higher than init_admin.py for 'export graph data'"
    );
    assert!(
        good_path > 0.5,
        "graph_data.py should match >50% of keywords"
    );
    assert_eq!(bad_path, 0.0, "init_admin.py should have 0 path match");
}

#[test]
fn test_fallback_score_path_dominates() {
    let desc = vec!["graph".to_string(), "data".to_string()];
    // Path with strong match should dominate even with 0 fn_name overlap
    let path_heavy = compute_path_score(&desc, "src/graph_data.py");
    // Weight: 65% path score means a 1.0 path score yields 0.65 minimum
    assert!(
        path_heavy * 0.65 > 0.3,
        "path-dominant score should exceed threshold: {}",
        path_heavy * 0.65
    );
}

#[test]
fn test_low_confidence_threshold() {
    // Scores below 0.3 should be treated as "no confident match"
    let desc = vec!["quantum".to_string(), "entanglement".to_string()];
    // Path with no real overlap should score well below 0.3
    let low = compute_path_score(&desc, "src/auth_handler.py");
    assert!(
        low < 0.3,
        "unrelated path should score below threshold: {}",
        low
    );
}

#[test]
fn test_majority_prefix_detection() {
    // 3/5 share "get_" prefix → majority → detected
    let names = vec![
        "get_user",
        "get_session",
        "get_token",
        "create_item",
        "delete_item",
    ];
    let prefix = detect_common_prefix(&names);
    assert_eq!(prefix, Some("get_".to_string()));

    // 2/5 share prefix → not majority → None
    let names2 = vec![
        "get_user",
        "get_session",
        "create_item",
        "delete_item",
        "check_auth",
    ];
    let prefix2 = detect_common_prefix(&names2);
    assert!(prefix2.is_none(), "non-majority prefix should be None");
}

#[test]
fn reuse_candidates_lead_create_suggestions() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&stored_node(1, "time", NodeKind::Module, "src/time.rs", 1))
        .unwrap();
    store
        .insert_node(&stored_node(
            2,
            "parse_timestamp",
            NodeKind::Function,
            "src/time.rs",
            1,
        ))
        .unwrap();

    let result = suggest_name(&store, "time timestamp", None, Some("function"));

    assert_eq!(result.reuse_candidates.len(), 1);
    assert_eq!(result.reuse_candidates[0].name, "parse_timestamp");
    assert_eq!(
        result.reuse_candidates[0].source,
        crate::types::ReuseCandidateSource::Lexical
    );
    assert!(result.reuse_candidates[0].score >= 0.4);
    assert!(!result.reuse_candidates[0].evidence.is_empty());
    assert!(!result.suggestions.is_empty());
}

#[test]
fn reuse_candidates_exclude_test_files() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&stored_node(
            1,
            "time_test",
            NodeKind::Module,
            "tests/time_test.rs",
            1,
        ))
        .unwrap();
    store
        .insert_node(&stored_node(
            2,
            "parse_timestamp",
            NodeKind::Function,
            "tests/time_test.rs",
            1,
        ))
        .unwrap();

    let result = suggest_name(&store, "parse timestamp", None, None);
    assert!(result.reuse_candidates.is_empty());
}

#[test]
fn semantic_candidates_are_opt_in_and_labeled_candidate_only() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&stored_node(1, "time", NodeKind::Module, "src/time.rs", 1))
        .unwrap();
    let mut existing = stored_node(2, "parse_timestamp", NodeKind::Function, "src/time.rs", 1);
    existing.docstring = None;
    store.insert_node(&existing).unwrap();

    let lexical = suggest_name(&store, "convert unix seconds", None, Some("function"));
    assert!(lexical.reuse_candidates.is_empty());

    let semantic = suggest_name_with_options(
        &store,
        "convert unix seconds",
        None,
        Some("function"),
        NameOptions {
            semantic_candidates: true,
        },
    );
    let candidate = semantic
        .reuse_candidates
        .iter()
        .find(|candidate| candidate.name == "parse_timestamp")
        .expect("concept expansion should nominate the timestamp parser");
    assert_eq!(
        candidate.source,
        crate::types::ReuseCandidateSource::Semantic
    );
    assert!(candidate.evidence[0].contains("never warning/gate"));

    let both = suggest_name_with_options(
        &store,
        "parse timestamp",
        None,
        Some("function"),
        NameOptions {
            semantic_candidates: true,
        },
    );
    assert_eq!(
        both.reuse_candidates[0].source,
        crate::types::ReuseCandidateSource::Lexical,
        "lexical evidence must win when both generators nominate the same hash"
    );
}
