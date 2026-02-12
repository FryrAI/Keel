// Tests for E003 missing docstring enforcement (Spec 006 - Enforcement Engine)
use keel_core::types::NodeKind;
use keel_enforce::violations::check_missing_docstring;
use keel_parsers::resolver::{Definition, FileIndex};

fn make_def(name: &str, is_public: bool, docstring: Option<&str>) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("def {name}(x: int) -> int"),
        file_path: "src/lib.py".to_string(),
        line_start: 1,
        line_end: 5,
        docstring: docstring.map(|s| s.to_string()),
        is_public,
        type_hints_present: true,
        body_text: "return x".to_string(),
    }
}

fn make_file(defs: Vec<Definition>) -> FileIndex {
    FileIndex {
        file_path: "src/lib.py".to_string(),
        content_hash: 0,
        definitions: defs,
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

#[test]
fn test_e003_public_function_missing_docstring() {
    let file = make_file(vec![make_def("process", true, None)]);
    let violations = check_missing_docstring(&file);

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E003");
    assert_eq!(violations[0].severity, "ERROR");
    assert_eq!(violations[0].category, "missing_docstring");
}

#[test]
fn test_e003_public_function_with_docstring_passes() {
    let file = make_file(vec![make_def("process", true, Some("Process the data."))]);
    let violations = check_missing_docstring(&file);

    assert!(violations.is_empty());
}

#[test]
fn test_e003_private_function_no_docstring_passes() {
    let file = make_file(vec![make_def("_helper", false, None)]);
    let violations = check_missing_docstring(&file);

    assert!(violations.is_empty());
}

#[test]
fn test_e003_class_not_checked() {
    let class_def = Definition {
        name: "MyClass".to_string(),
        kind: NodeKind::Class,
        signature: "class MyClass".to_string(),
        file_path: "src/lib.py".to_string(),
        line_start: 1,
        line_end: 10,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "pass".to_string(),
    };
    let file = make_file(vec![class_def]);
    let violations = check_missing_docstring(&file);

    assert!(violations.is_empty());
}

#[test]
fn test_e003_includes_fix_hint() {
    let file = make_file(vec![make_def("calculate", true, None)]);
    let violations = check_missing_docstring(&file);

    assert_eq!(violations.len(), 1);
    let hint = violations[0].fix_hint.as_ref().unwrap();
    assert!(hint.contains("calculate"));
}

#[test]
fn test_e003_multiple_undocumented() {
    let file = make_file(vec![
        make_def("func_a", true, None),
        make_def("func_b", true, Some("Documented.")),
        make_def("func_c", true, None),
    ]);
    let violations = check_missing_docstring(&file);

    assert_eq!(violations.len(), 2);
    let msgs: Vec<&str> = violations.iter().map(|v| v.message.as_str()).collect();
    assert!(msgs.iter().any(|m| m.contains("func_a")));
    assert!(msgs.iter().any(|m| m.contains("func_c")));
}

#[test]
fn test_e003_file_and_line() {
    let file = make_file(vec![make_def("nodoc", true, None)]);
    let violations = check_missing_docstring(&file);

    assert_eq!(violations[0].file, "src/lib.py");
    assert_eq!(violations[0].line, 1);
}

#[test]
fn test_e003_confidence_is_1() {
    let file = make_file(vec![make_def("nodoc", true, None)]);
    let violations = check_missing_docstring(&file);

    assert_eq!(violations[0].confidence, 1.0);
}
