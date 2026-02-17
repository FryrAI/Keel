/// Shared helper for JSON schema validation used by all schema contract tests.
pub fn validate_against_schema(json_value: &serde_json::Value, schema_str: &str) {
    let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
    let validator = jsonschema::validator_for(&schema)
        .expect("Failed to compile JSON schema");
    let errors: Vec<_> = validator.iter_errors(json_value).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors
            .iter()
            .map(|e| format!("  - {} (at {})", e, e.instance_path))
            .collect();
        panic!(
            "JSON schema validation failed:\n{}",
            msgs.join("\n")
        );
    }
}
