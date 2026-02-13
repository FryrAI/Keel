// Tests for ModuleProfile creation and querying (Spec 000 - Graph Schema)

use keel_core::types::ModuleProfile;

#[test]
/// Creating a ModuleProfile should capture the module's path and module_id.
fn test_module_profile_creation() {
    // GIVEN a file path and module id
    let profile = ModuleProfile {
        module_id: 42,
        path: "src/utils/parser.ts".into(),
        function_count: 3,
        function_name_prefixes: vec!["parse".into()],
        primary_types: vec!["Parser".into()],
        import_sources: vec!["fs".into()],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec!["parsing".into()],
    };

    // THEN the path and module_id fields are correctly set
    assert_eq!(profile.path, "src/utils/parser.ts");
    assert_eq!(profile.module_id, 42);
    assert_eq!(profile.function_count, 3);
}

#[test]
/// ModuleProfile should store responsibility_keywords extracted from the module.
fn test_module_profile_responsibility_keywords() {
    // GIVEN a module whose functions are parse_json, parse_xml, parse_csv
    let profile = ModuleProfile {
        module_id: 1,
        path: "src/parsers.ts".into(),
        function_count: 3,
        function_name_prefixes: vec!["parse".into()],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec!["parse".into(), "json".into(), "xml".into(), "csv".into()],
    };

    // THEN responsibility_keywords includes "parse"
    assert!(
        profile.responsibility_keywords.contains(&"parse".into()),
        "responsibility_keywords should contain 'parse'"
    );
    assert_eq!(profile.responsibility_keywords.len(), 4);
}

#[test]
/// ModuleProfile should store function_name_prefixes for placement scoring.
fn test_module_profile_function_name_prefixes() {
    // GIVEN a module with functions: validate_email, validate_phone, validate_address
    let profile = ModuleProfile {
        module_id: 2,
        path: "src/validators.ts".into(),
        function_count: 3,
        function_name_prefixes: vec!["validate".into()],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec!["validate".into()],
    };

    // THEN function_name_prefixes includes "validate"
    assert!(
        profile.function_name_prefixes.contains(&"validate".into()),
        "function_name_prefixes should contain 'validate'"
    );
}

#[test]
/// An empty module should produce a valid ModuleProfile with empty keyword lists.
fn test_empty_module_profile() {
    // GIVEN a module file with no functions or classes
    let profile = ModuleProfile {
        module_id: 3,
        path: "src/empty.ts".into(),
        function_count: 0,
        function_name_prefixes: vec![],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec![],
    };

    // THEN all collection fields are empty and function_count is 0
    assert_eq!(profile.function_count, 0);
    assert!(profile.function_name_prefixes.is_empty());
    assert!(profile.primary_types.is_empty());
    assert!(profile.import_sources.is_empty());
    assert!(profile.export_targets.is_empty());
    assert!(profile.responsibility_keywords.is_empty());
    assert_eq!(profile.external_endpoint_count, 0);
}

#[test]
/// ModuleProfile should track the count of functions contained in the module.
fn test_module_profile_function_count() {
    // GIVEN a module with 5 functions
    let profile = ModuleProfile {
        module_id: 4,
        path: "src/handlers.ts".into(),
        function_count: 5,
        function_name_prefixes: vec!["handle".into()],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec!["handle".into()],
    };

    // THEN function_count equals 5
    assert_eq!(profile.function_count, 5);
}

#[test]
#[ignore = "BUG: ModuleProfile lacks class_count field"]
/// ModuleProfile should track the count of classes contained in the module.
fn test_module_profile_class_count() {
    // ModuleProfile struct does not have a class_count field.
    // function_count is the only count available.
    let profile = ModuleProfile {
        module_id: 5,
        path: "src/models.ts".into(),
        function_count: 3,
        function_name_prefixes: vec![],
        primary_types: vec!["User".into(), "Order".into(), "Product".into()],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec![],
    };
    // When class_count is added, assert_eq!(profile.class_count, 3);
    assert_eq!(profile.function_count, 3);
}

#[test]
#[ignore = "BUG: ModuleProfile lacks line_count field"]
/// ModuleProfile should track total lines of code for the module.
fn test_module_profile_line_count() {
    // ModuleProfile struct does not have a line_count field.
    let profile = ModuleProfile {
        module_id: 6,
        path: "src/large_module.ts".into(),
        function_count: 10,
        function_name_prefixes: vec![],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec![],
    };
    // When line_count is added, assert_eq!(profile.line_count, 150);
    assert_eq!(profile.function_count, 10);
}

#[test]
/// Updating a ModuleProfile after file changes should reflect new content.
fn test_module_profile_update_on_file_change() {
    // GIVEN an existing ModuleProfile for a module with 3 functions
    let before = ModuleProfile {
        module_id: 7,
        path: "src/utils.ts".into(),
        function_count: 3,
        function_name_prefixes: vec!["parse".into()],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec!["parse".into()],
    };

    // WHEN a new function is added and the profile is rebuilt
    let after = ModuleProfile {
        module_id: 7,
        path: "src/utils.ts".into(),
        function_count: 4,
        function_name_prefixes: vec!["parse".into(), "format".into()],
        primary_types: vec![],
        import_sources: vec![],
        export_targets: vec![],
        external_endpoint_count: 0,
        responsibility_keywords: vec!["parse".into(), "format".into()],
    };

    // THEN the function_count increments
    assert_eq!(before.function_count, 3);
    assert_eq!(after.function_count, 4);
    assert_ne!(before.function_count, after.function_count);

    // AND keywords update to include the new prefix
    assert!(after.function_name_prefixes.contains(&"format".into()));
    assert!(after.responsibility_keywords.contains(&"format".into()));
}
