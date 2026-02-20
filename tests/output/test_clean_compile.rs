// Tests for clean compile output behavior (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::human::HumanFormatter;
use keel_output::json::JsonFormatter;
use keel_output::llm::LlmFormatter;
use keel_output::OutputFormatter;

fn clean_compile() -> CompileResult {
    CompileResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "compile".into(),
        status: "ok".into(),
        files_analyzed: vec!["src/main.rs".into(), "src/lib.rs".into()],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 5,
            edges_updated: 3,
            hashes_changed: vec![],
        },
    }
}

#[test]
fn test_clean_compile_json() {
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&clean_compile());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["errors"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["warnings"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["files_analyzed"].as_array().unwrap().len(), 2);
}

#[test]
fn test_clean_compile_llm() {
    let fmt = LlmFormatter::new();
    let out = fmt.format_compile(&clean_compile());

    // Clean compile = empty string for LLM format
    assert!(out.is_empty(), "LLM clean compile must be empty string");
}

#[test]
fn test_clean_compile_human() {
    let fmt = HumanFormatter;
    let out = fmt.format_compile(&clean_compile());

    // Clean compile = empty stdout for human format
    assert!(out.is_empty(), "Human clean compile must be empty stdout");
}

#[test]
fn test_clean_compile_verbose() {
    // JSON format always includes info block (verbose context)
    let fmt = JsonFormatter;
    let out = fmt.format_compile(&clean_compile());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    // Info block present even for clean compile in JSON
    assert!(parsed["info"].is_object());
    assert_eq!(parsed["info"]["nodes_updated"], 5);
    assert_eq!(parsed["info"]["edges_updated"], 3);
}

#[test]
fn test_clean_compile_silent_without_verbose() {
    // LLM and Human formats produce empty output on clean compile
    // This is critical for LLM agents that parse stdout
    let llm_fmt = LlmFormatter::new();
    let human_fmt = HumanFormatter;
    let result = clean_compile();

    let llm_out = llm_fmt.format_compile(&result);
    let human_out = human_fmt.format_compile(&result);

    assert!(
        llm_out.is_empty(),
        "LLM format must be silent on clean compile"
    );
    assert!(
        human_out.is_empty(),
        "Human format must be silent on clean compile"
    );
}
