// Tests for E002 missing type hints enforcement (Spec 006 - Enforcement Engine)
use keel_core::types::NodeKind;
use keel_enforce::violations::check_missing_type_hints;
use keel_parsers::resolver::{Definition, FileIndex};

fn make_def(name: &str, is_public: bool, type_hints: bool) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: if type_hints {
            format!("def {name}(data: list) -> dict")
        } else {
            format!("def {name}(data)")
        },
        file_path: "src/main.py".to_string(),
        line_start: 1,
        line_end: 5,
        docstring: None,
        is_public,
        type_hints_present: type_hints,
        body_text: "return data".to_string(),
    }
}

fn make_file(defs: Vec<Definition>) -> FileIndex {
    FileIndex {
        file_path: "src/main.py".to_string(),
        content_hash: 0,
        definitions: defs,
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

#[test]
fn test_e002_python_missing_type_hints() {
    let file = make_file(vec![make_def("process", true, false)]);
    let violations = check_missing_type_hints(&file);

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].code, "E002");
    assert_eq!(violations[0].severity, "ERROR");
    assert_eq!(violations[0].category, "missing_type_hints");
    assert!(violations[0].fix_hint.is_some());
}

#[test]
fn test_e002_python_with_type_hints_passes() {
    let file = make_file(vec![make_def("process", true, true)]);
    let violations = check_missing_type_hints(&file);

    assert!(violations.is_empty());
}

#[test]
fn test_e002_private_function_no_violation() {
    let file = make_file(vec![make_def("_helper", false, false)]);
    let violations = check_missing_type_hints(&file);

    assert!(violations.is_empty());
}

#[test]
fn test_e002_class_not_checked() {
    let class_def = Definition {
        name: "MyClass".to_string(),
        kind: NodeKind::Class,
        signature: "class MyClass".to_string(),
        file_path: "src/main.py".to_string(),
        line_start: 1,
        line_end: 10,
        docstring: None,
        is_public: true,
        type_hints_present: false,
        body_text: "pass".to_string(),
    };
    let file = make_file(vec![class_def]);
    let violations = check_missing_type_hints(&file);

    assert!(violations.is_empty());
}

#[test]
fn test_e002_includes_fix_hint() {
    let file = make_file(vec![make_def("compute", true, false)]);
    let violations = check_missing_type_hints(&file);

    assert_eq!(violations.len(), 1);
    let hint = violations[0].fix_hint.as_ref().unwrap();
    assert!(hint.contains("compute"));
}

#[test]
fn test_e002_multiple_functions() {
    let file = make_file(vec![
        make_def("func_a", true, false),
        make_def("func_b", true, false),
        make_def("func_c", true, true),
    ]);
    let violations = check_missing_type_hints(&file);

    assert_eq!(violations.len(), 2);
    let msgs: Vec<&str> = violations.iter().map(|v| v.message.as_str()).collect();
    assert!(msgs.iter().any(|m| m.contains("func_a")));
    assert!(msgs.iter().any(|m| m.contains("func_b")));
}

#[test]
fn test_e002_file_location_correct() {
    let file = make_file(vec![make_def("broken", true, false)]);
    let violations = check_missing_type_hints(&file);

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].file, "src/main.py");
    assert_eq!(violations[0].line, 1);
}

#[test]
fn test_e002_confidence_is_1() {
    let file = make_file(vec![make_def("notype", true, false)]);
    let violations = check_missing_type_hints(&file);

    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].confidence, 1.0);
}
