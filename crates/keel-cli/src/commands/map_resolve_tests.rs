use super::*;

fn make_module_ids(entries: &[(&str, u64)]) -> HashMap<String, u64> {
    entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

#[test]
fn test_resolve_go_import_to_module() {
    let modules = make_module_ids(&[
        ("cmd/root.go", 1),
        ("internal/cobra/command.go", 2),
        ("pkg/utils/helper.go", 3),
    ]);
    let result = resolve_import_to_module("github.com/spf13/cobra", &modules);
    assert_eq!(result, Some(2));

    let result = resolve_import_to_module("github.com/myorg/mylib/utils", &modules);
    assert_eq!(result, Some(3));
}

#[test]
fn test_resolve_rust_crate_import_to_module() {
    let modules = make_module_ids(&[
        ("src/store.rs", 10),
        ("src/main.rs", 11),
        ("src/hash/mod.rs", 12),
    ]);
    let result = resolve_import_to_module("crate::store::GraphStore", &modules);
    assert_eq!(result, Some(10));

    let result = resolve_import_to_module("crate::hash", &modules);
    assert_eq!(result, Some(12));
}

#[test]
fn test_resolve_rust_workspace_crate_import() {
    let modules = make_module_ids(&[
        ("crates/keel-core/src/store.rs", 20),
        ("crates/keel-core/src/types.rs", 21),
    ]);
    let result = resolve_import_to_module("crate::store::GraphStore", &modules);
    assert_eq!(result, Some(20));
}

#[test]
fn test_resolve_exact_match() {
    let modules = make_module_ids(&[("src/lib.rs", 5)]);
    let result = resolve_import_to_module("src/lib.rs", &modules);
    assert_eq!(result, Some(5));
}

#[test]
fn test_resolve_relative_ts_import() {
    let modules = make_module_ids(&[("utils.ts", 7), ("components/index.ts", 8)]);
    let result = resolve_import_to_module("./utils", &modules);
    assert_eq!(result, Some(7));

    let result = resolve_import_to_module("./components", &modules);
    assert_eq!(result, Some(8));
}

#[test]
fn test_resolve_unknown_import_returns_none() {
    let modules = make_module_ids(&[("src/main.rs", 1)]);
    let result = resolve_import_to_module("std::collections::HashMap", &modules);
    assert_eq!(result, None);

    let result = resolve_import_to_module("react", &modules);
    assert_eq!(result, None);
}

#[test]
fn test_resolve_package_import_npm() {
    let mut pkg_index: HashMap<String, HashMap<String, u64>> = HashMap::new();
    let mut shared_fns = HashMap::new();
    shared_fns.insert("formatDate".to_string(), 42u64);
    shared_fns.insert("parseDate".to_string(), 43u64);
    pkg_index.insert("shared".to_string(), shared_fns);

    // Direct package name match
    let result = resolve_package_import("formatDate", "shared", &pkg_index);
    assert!(result.is_some());
    let (id, conf) = result.unwrap();
    assert_eq!(id, 42);
    assert!(conf < 0.80); // lower confidence for cross-package

    // Scoped package: @myorg/shared -> shared
    let result = resolve_package_import("parseDate", "@myorg/shared", &pkg_index);
    assert!(result.is_some());
    assert_eq!(result.unwrap().0, 43);
}

#[test]
fn test_resolve_package_import_go() {
    let mut pkg_index: HashMap<String, HashMap<String, u64>> = HashMap::new();
    let mut utils_fns = HashMap::new();
    utils_fns.insert("FormatTime".to_string(), 100u64);
    pkg_index.insert("utils".to_string(), utils_fns);

    // Go module path: last segment is package name
    let result = resolve_package_import("FormatTime", "github.com/myorg/repo/utils", &pkg_index);
    assert!(result.is_some());
    assert_eq!(result.unwrap().0, 100);
}

#[test]
fn test_resolve_package_import_not_found() {
    let pkg_index: HashMap<String, HashMap<String, u64>> = HashMap::new();
    let result = resolve_package_import("missing", "unknown-pkg", &pkg_index);
    assert!(result.is_none());
}

#[test]
fn test_build_package_node_index() {
    let mut global: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    global.insert(
        "doWork".to_string(),
        vec![
            ("packages/core/src/worker.ts".to_string(), 10),
            ("packages/api/src/handler.ts".to_string(), 20),
        ],
    );

    let mut file_packages = HashMap::new();
    file_packages.insert(
        "packages/core/src/worker.ts".to_string(),
        "core".to_string(),
    );
    file_packages.insert("packages/api/src/handler.ts".to_string(), "api".to_string());

    let index = build_package_node_index(&global, &file_packages);
    assert_eq!(index.get("core").unwrap().get("doWork"), Some(&10));
    assert_eq!(index.get("api").unwrap().get("doWork"), Some(&20));
}

fn def(
    name: &str,
    kind: keel_core::types::NodeKind,
    start: u32,
    end: u32,
) -> keel_parsers::resolver::Definition {
    keel_parsers::resolver::Definition {
        name: name.to_string(),
        kind,
        signature: name.to_string(),
        file_path: "f.rs".to_string(),
        line_start: start,
        line_end: end,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: String::new(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
    }
}

#[test]
fn test_find_containing_def_attributes_to_innermost_function() {
    use keel_core::types::NodeKind;
    // A method (5..15) nested in a class (1..30); a call on line 8 belongs to
    // the method (the innermost, smallest-span def), never the class or module.
    let defs = vec![
        def("MyClass", NodeKind::Class, 1, 30),
        def("my_method", NodeKind::Function, 5, 15),
    ];
    let mut name_to_id = HashMap::new();
    name_to_id.insert(("f.rs".to_string(), "MyClass".to_string()), 100u64);
    name_to_id.insert(("f.rs".to_string(), "my_method".to_string()), 101u64);

    let got = find_containing_def(&defs, 8, "f.rs", &name_to_id, Some(1));
    assert_eq!(got, Some(101), "call attributes to the enclosing method");
}

#[test]
fn test_find_containing_def_falls_back_to_module() {
    use keel_core::types::NodeKind;
    // A top-level reference (line 40) outside any function falls back to the
    // file's path-named module id, never to a stray module-kind def.
    let defs = vec![def("helper", NodeKind::Function, 5, 15)];
    let name_to_id = HashMap::new();
    let got = find_containing_def(&defs, 40, "f.rs", &name_to_id, Some(7));
    assert_eq!(got, Some(7), "top-level ref attributes to the module");
}

#[test]
fn test_resolve_same_file_method_self_high_confidence() {
    use keel_core::types::NodeKind;
    let defs = vec![def("run", NodeKind::Function, 1, 3)];
    let mut name_to_id = HashMap::new();
    name_to_id.insert(("f.rs".to_string(), "run".to_string()), 200u64);

    let got = resolve_same_file_method("self.run", "f.rs", &defs, &name_to_id);
    assert_eq!(got, Some((200, 0.9)), "self.method binds at 0.9");
    let got = resolve_same_file_method("this.run", "f.rs", &defs, &name_to_id);
    assert_eq!(got, Some((200, 0.9)), "this.method binds at 0.9");
}

#[test]
fn test_resolve_same_file_method_obj_unique_warning_tier() {
    use keel_core::types::NodeKind;
    let defs = vec![def("run", NodeKind::Function, 1, 3)];
    let mut name_to_id = HashMap::new();
    name_to_id.insert(("f.rs".to_string(), "run".to_string()), 201u64);

    let got = resolve_same_file_method("obj.run", "f.rs", &defs, &name_to_id);
    assert_eq!(
        got,
        Some((201, 0.7)),
        "unfamiliar receiver, unique target, resolves at warning-tier 0.7"
    );
}

#[test]
fn test_resolve_same_file_method_obj_ambiguous_is_none() {
    use keel_core::types::NodeKind;
    // Two same-named same-file defs: an unfamiliar receiver stays unresolved.
    let defs = vec![
        def("run", NodeKind::Function, 1, 3),
        def("run", NodeKind::Function, 10, 12),
    ];
    let mut name_to_id = HashMap::new();
    name_to_id.insert(("f.rs".to_string(), "run".to_string()), 202u64);

    let got = resolve_same_file_method("obj.run", "f.rs", &defs, &name_to_id);
    assert_eq!(got, None, "ambiguous non-self receiver does not resolve");
}

#[test]
fn test_extract_package_name_variants() {
    assert_eq!(
        extract_package_name("@scope/name"),
        Some("name".to_string())
    );
    assert_eq!(
        extract_package_name("keel_core::types"),
        Some("keel_core".to_string())
    );
    assert_eq!(
        extract_package_name("github.com/org/repo/pkg"),
        Some("pkg".to_string())
    );
    assert_eq!(extract_package_name("lodash"), Some("lodash".to_string()));
    assert_eq!(extract_package_name("./local"), None);
    // crate/super/self are not external packages
    assert_eq!(extract_package_name("crate::store"), None);
    assert_eq!(extract_package_name("super::sibling"), None);
}
