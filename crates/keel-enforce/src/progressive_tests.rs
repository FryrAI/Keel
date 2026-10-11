//! Existing tests pin the no-Git rule; Git tests pin the commit baseline.
use super::*;
use keel_core::types::NodeKind;
use keel_parsers::resolver::Definition;

fn undocumented_def(name: &str, file: &str) -> Definition {
    Definition {
        complexity: 1,
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 10,
        line_end: 20,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "do_work()".to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
        is_trivial_wrapper_body: false,
    }
}

fn file_with(def: Definition) -> FileIndex {
    FileIndex {
        file_path: def.file_path.clone(),
        content_hash: 0,
        definitions: vec![def],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

/// A stored node whose hash matches the definition exactly (untouched).
fn matching_node(def: &Definition) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id: 1,
        hash: keel_core::hash::compute_hash(
            &def.signature,
            &def.body_for_hash(),
            def.docstring.as_deref().unwrap_or(""),
        ),
        kind: NodeKind::Function,
        name: def.name.clone(),
        signature: def.signature.clone(),
        file_path: def.file_path.clone(),
        line_start: def.line_start,
        line_end: def.line_end,
        docstring: def.docstring.clone(),
        is_public: def.is_public,
        type_hints_present: def.type_hints_present,
        has_docstring: def.docstring.is_some(),
        is_associated: def.is_associated,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

fn e003_violation(def: &Definition) -> Violation {
    Violation {
        code: "E003".to_string(),
        severity: "ERROR".to_string(),
        category: "missing_docstring".to_string(),
        message: format!("Public function `{}` has no docstring", def.name),
        file: def.file_path.clone(),
        line: def.line_start,
        hash: String::new(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".to_string(),
        fix_hint: Some("Add a documentation comment".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

#[test]
fn untouched_function_downgrades_to_warning() {
    let def = undocumented_def("legacy", "src/old.rs");
    let node = matching_node(&def);
    let file = file_with(def.clone());

    let out = apply_progressive_adoption(vec![e003_violation(&def)], &file, &[node], None);
    assert_eq!(out[0].severity, "WARNING");
    assert!(out[0].fix_hint.as_deref().unwrap().contains("pre-existing"));
}

#[test]
fn modified_function_stays_error() {
    let def = undocumented_def("edited", "src/old.rs");
    let mut node = matching_node(&def);
    node.hash = "differentHash".to_string(); // stored state differs → touched
    let file = file_with(def.clone());

    let out = apply_progressive_adoption(vec![e003_violation(&def)], &file, &[node], None);
    assert_eq!(out[0].severity, "ERROR");
}

#[test]
fn new_function_stays_error() {
    let def = undocumented_def("brand_new", "src/new.rs");
    let file = file_with(def.clone());

    let out = apply_progressive_adoption(vec![e003_violation(&def)], &file, &[], None);
    assert_eq!(out[0].severity, "ERROR");
}

#[test]
fn structural_codes_never_downgrade() {
    let def = undocumented_def("legacy", "src/old.rs");
    let node = matching_node(&def);
    let file = file_with(def.clone());

    let mut v = e003_violation(&def);
    v.code = "E001".to_string();
    v.category = "broken_caller".to_string();
    let out = apply_progressive_adoption(vec![v], &file, &[node], None);
    assert_eq!(out[0].severity, "ERROR", "E001 must never be grandfathered");
}

fn git_repo(text: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.com"],
    ] {
        git(dir.path(), &args);
    }
    std::fs::write(dir.path().join("legacy.py"), text).unwrap();
    git(dir.path(), &["add", "legacy.py"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    dir
}

fn git(root: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn parse_working(root: &Path, text: &str) -> FileIndex {
    std::fs::write(root.join("legacy.py"), text).unwrap();
    BlobParser::new().parse("legacy.py", text).unwrap()
}

#[test]
fn pasted_same_named_twin_is_new() {
    let definition = "def run(x):\n    return x\n\n";
    let dir = git_repo(&definition.repeat(2));
    let file = parse_working(dir.path(), &definition.repeat(3));
    assert_eq!(file.definitions.len(), 3);
    let violations = file.definitions.iter().map(e003_violation).collect();
    let mut base = GitBase::new(dir.path(), "HEAD");
    let out = apply_progressive_adoption(violations, &file, &[], Some(&mut base));
    assert_eq!(
        out.iter().map(|v| v.severity.as_str()).collect::<Vec<_>>(),
        ["WARNING", "WARNING", "ERROR"]
    );
}

#[test]
fn unreadable_base_never_downgrades() {
    let text = "def legacy(x):\n    return x\n";
    let dir = git_repo(text);
    let file = parse_working(dir.path(), text);
    let def = &file.definitions[0];
    let node = matching_node(def);
    let mut base = GitBase::new(dir.path(), "HEAD");
    base.commit = Some(Ok("unavailable-base".to_string()));
    let out =
        apply_progressive_adoption(vec![e003_violation(def)], &file, &[node], Some(&mut base));
    assert_eq!(out[0].severity, "ERROR");
    assert!(out[0]
        .fix_hint
        .as_deref()
        .is_some_and(|hint| !hint.contains("pre-existing")));
}

#[test]
fn nothing_to_judge_runs_no_git() {
    let dir = git_repo("def legacy(x):\n    return x\n");
    let def = undocumented_def("legacy", "legacy.py");
    let file = file_with(def.clone());
    let mut base = GitBase::new(dir.path(), "HEAD");
    let mut warning = e003_violation(&def);
    warning.severity = "WARNING".to_string();
    let mut structural = e003_violation(&def);
    structural.code = "E001".to_string();
    let out = apply_progressive_adoption(vec![warning, structural], &file, &[], Some(&mut base));
    assert_eq!(out[0].severity, "WARNING");
    assert_eq!(out[1].severity, "ERROR");
    assert!(base.commit.is_none());
    assert!(base.renames.is_none());
}

#[test]
fn crlf_working_tree_against_lf_blob_is_pre_existing() {
    let text = "def legacy(\n    first,\n    second,\n):\n    return first + second\n";
    let dir = git_repo(text);
    let file = parse_working(dir.path(), &text.replace('\n', "\r\n"));
    assert_eq!(file.definitions.len(), 1);
    let mut base = GitBase::new(dir.path(), "HEAD");
    let out = apply_progressive_adoption(
        vec![e003_violation(&file.definitions[0])],
        &file,
        &[],
        Some(&mut base),
    );
    assert_eq!(out[0].severity, "WARNING");
    assert!(out[0].fix_hint.as_deref().unwrap().contains("pre-existing"));
}
