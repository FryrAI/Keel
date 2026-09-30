//! Round-two regressions for live compile candidates and Tier-3 admission.

use std::fs;

use protobuf::Message;
use scip::types::{Document, Index, Occurrence};

use super::test_bare_call_binding::{fixture, incoming_calls};
use crate::common::{assert_no_violation, compile_json, keel};

#[test]
fn typescript_scripts_free_function_wins_over_member_in_either_order() {
    let free = "/** Free. */\nfunction run(a: number): void {}\n";
    let member = "/** Guard. */\nclass Guard {\n /** Member. */\n run(): void {}\n}\n";
    let caller = "/** Caller. */\nfunction wire(): void { run(1); }\n";
    for extension in ["tsx", "jsx", "svelte", "astro"] {
        for (first, second) in [(free, member), (member, free)] {
            let file = format!("src/lib.{extension}");
            let source = format!("{first}{second}{caller}");
            let source = if extension == "svelte" {
                format!("<script lang=\"ts\">\n{source}</script>\n")
            } else if extension == "astro" {
                format!("---\n{source}---\n")
            } else {
                source
            };
            let dir = fixture(&[(&file, &source)]);
            assert_eq!(incoming_calls(dir.path(), &file, "run", true), 0);
            assert_eq!(incoming_calls(dir.path(), &file, "run", false), 1);
            fs::write(
                dir.path().join(&file),
                source.replace("run(1)", "run(1, 2)"),
            )
            .unwrap();
            let result = compile_json(dir.path(), &file);
            assert_eq!(
                crate::common::violations_with_code(&result, "E005").len(),
                1,
                "{extension}: {result}"
            );
            assert_eq!(incoming_calls(dir.path(), &file, "run", true), 0);
            assert_eq!(incoming_calls(dir.path(), &file, "run", false), 1);
        }
    }
}

#[test]
fn typescript_scripts_render_call_never_binds_class_member() {
    for extension in ["tsx", "jsx", "svelte", "astro"] {
        let file = format!("src/entry.{extension}");
        let source = "import { render } from \"react-dom\";\n/** App. */\nclass App {\n /** Renders. */\n render() {}\n}\nrender(<App />, el);\n";
        let source = if extension == "svelte" {
            // Svelte script blocks use the TS grammar, not JSX.
            format!("<script>\n{}</script>\n", source.replace("<App />", "App"))
        } else if extension == "astro" {
            format!("---\n{}---\n", source.replace("<App />", "App"))
        } else {
            source.to_string()
        };
        let dir = fixture(&[(&file, &source)]);
        assert_eq!(incoming_calls(dir.path(), &file, "render", true), 0);
        fs::write(dir.path().join(&file), format!("{source}\n")).unwrap();
        let result = compile_json(dir.path(), &file);
        assert_no_violation(&result, "E005");
        assert_eq!(incoming_calls(dir.path(), &file, "render", true), 0);
        fs::write(
            dir.path().join(&file),
            source.replace("render() {}", "render(a, b, c) {}"),
        )
        .unwrap();
        let result = compile_json(dir.path(), &file);
        assert_no_violation(&result, "E001");
        assert_no_violation(&result, "E005");
        fs::write(
            dir.path().join(&file),
            source.replace(" /** Renders. */\n render() {}\n", ""),
        )
        .unwrap();
        let result = compile_json(dir.path(), &file);
        assert_no_violation(&result, "E004");
    }
}

#[test]
fn added_second_free_function_disables_stored_local_replacement() {
    let source = "/// Free.\npub fn run(a: i32) { let _ = a; }\n/// Guard.\npub struct Guard;\nimpl Guard {\n /// Member.\n pub fn run(&self) {}\n}\n";
    let dir = fixture(&[("src/lib.rs", source)]);
    let added = "/// Inner.\npub mod inner {\n /// Inner free.\n pub fn run(a: i32, b: i32) { let _ = (a,b); }\n /// Calls inner.\n pub fn wire() { run(1, 2); }\n}\n";
    fs::write(dir.path().join("src/lib.rs"), format!("{source}{added}")).unwrap();
    for _ in 0..2 {
        let result = compile_json(dir.path(), "src/lib.rs");
        assert_no_violation(&result, "E005");
        assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", false), 0);
        assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", true), 0);
    }
}

#[test]
fn newly_added_cfg_variants_keep_first_compile_free_binding() {
    let dir = fixture(&[("src/lib.rs", "/// Caller.\npub fn wire() {}\n")]);
    let source = "#[cfg(unix)]\nfn plat() {}\n#[cfg(not(unix))]\nfn plat() {}\n/// Caller.\npub fn wire() { plat(); }\n";
    fs::write(dir.path().join("src/lib.rs"), source).unwrap();
    let result = compile_json(dir.path(), "src/lib.rs");
    assert_no_violation(&result, "E005");
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "plat", false), 1);
}

#[test]
fn newly_added_python_branches_keep_first_compile_free_binding() {
    let dir = fixture(&[(
        "src/lib.py",
        "def wire() -> None:\n    \"\"\"Caller.\"\"\"\n    pass\n",
    )]);
    let source = "if flag:\n    def _load() -> None:\n        \"\"\"First.\"\"\"\n        pass\nelse:\n    def _load() -> None:\n        \"\"\"Second.\"\"\"\n        pass\ndef wire() -> None:\n    \"\"\"Caller.\"\"\"\n    _load()\n";
    fs::write(dir.path().join("src/lib.py"), source).unwrap();
    let result = compile_json(dir.path(), "src/lib.py");
    assert_no_violation(&result, "E005");
    assert_eq!(incoming_calls(dir.path(), "src/lib.py", "_load", false), 1);
}

#[test]
fn newly_added_nested_module_functions_keep_first_compile_base_target() {
    let dir = fixture(&[("src/lib.rs", "/// Caller.\npub fn wire() {}\n")]);
    let source = "mod a {\n pub fn run(x: i32) {}\n}\nmod b {\n pub fn run() {}\n}\n/// Caller.\npub fn wire() { run(1); }\n";
    fs::write(dir.path().join("src/lib.rs"), source).unwrap();
    let result = compile_json(dir.path(), "src/lib.rs");
    assert_no_violation(&result, "E005");
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "run", false), 1);
    use keel_core::store::GraphStore;
    let store = keel_core::sqlite::SqliteGraphStore::open(
        dir.path().join(".keel/graph.db").to_str().unwrap(),
    )
    .unwrap();
    let target = store
        .get_nodes_in_file("src/lib.rs")
        .into_iter()
        .find(|n| n.name == "run")
        .unwrap();
    assert_eq!(
        target.line_start, 2,
        "base inserts the first new definition"
    );
}

#[test]
fn tier3_full_map_does_not_replace_rejected_member_with_cross_file_free_function() {
    let dir = fixture(&[
        ("src/a.rs", "pub use crate::b::run;\n/// Guard.\npub struct Guard;\nimpl Guard {\n /// Member.\n pub fn run(&self) {}\n}\n"),
        ("src/b.rs", "/// Free.\npub fn run(x: i32) {}\n"),
        ("src/caller.rs", "use crate::a::run;\n/// Caller.\npub fn wire() {\n run(1);\n}\n"),
        ("src/nested/other.rs", "/// Caller with no earlier binding.\npub fn other() {\n run(1);\n}\n"),
    ]);
    // Calls must be on separate lines from declarations: Tier-3 admission
    // treats even a same-line contains edge as evidence of prior resolution.
    // The smallest SCIP fixture: one definition and two call occurrences.
    // The nested caller has no import or same-directory match, proving that
    // Tier 3 ran and may still bind an ordinary miss.
    let mut index = Index::new();
    let mut target = Document::new();
    target.relative_path = "src/b.rs".into();
    let symbol = "scip-rust cargo fixture 0.1.0 run().";
    let mut definition = Occurrence::new();
    definition.range = vec![1, 0, 3];
    definition.symbol = symbol.into();
    definition.symbol_roles = 1;
    target.occurrences.push(definition);
    for (file, line) in [("src/caller.rs", 3), ("src/nested/other.rs", 2)] {
        let mut caller = Document::new();
        caller.relative_path = file.into();
        let mut reference = Occurrence::new();
        reference.range = vec![line, 0, 3];
        reference.symbol = symbol.into();
        caller.occurrences.push(reference);
        index.documents.push(caller);
    }
    index.documents.push(target);
    fs::write(
        dir.path().join("index.scip"),
        index.write_to_bytes().unwrap(),
    )
    .unwrap();
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["tier3"]["enabled"] = true.into();
    config["tier3"]["scip_paths"]["rust"] = "index.scip".into();
    fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let out = keel(dir.path(), &["map", "--verbose"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("tier3 resolved 1 additional references"),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(incoming_calls(dir.path(), "src/a.rs", "run", true), 0);
    assert_eq!(incoming_calls(dir.path(), "src/b.rs", "run", false), 1);
    use keel_core::store::GraphStore;
    use keel_core::types::{EdgeDirection, EdgeKind};
    let store = keel_core::sqlite::SqliteGraphStore::open(
        dir.path().join(".keel/graph.db").to_str().unwrap(),
    )
    .unwrap();
    let wire = store
        .get_nodes_in_file("src/caller.rs")
        .into_iter()
        .find(|n| n.name == "wire")
        .unwrap();
    assert!(!store
        .get_edges(wire.id, EdgeDirection::Outgoing)
        .iter()
        .any(|e| e.kind == EdgeKind::Calls));
}

#[test]
fn tier3_rejected_member_keeps_same_line_helper_unresolved() {
    for same_file_member in [false, true] {
        let member =
            "/// Guard.\npub struct Guard;\nimpl Guard {\n /// Member.\n pub fn run(&self) {}\n}\n";
        let caller = "/// Caller.\npub fn wire() {\n run(1); helper(2);\n}\n";
        let caller = if same_file_member {
            format!("{member}{caller}")
        } else {
            caller.to_string()
        };
        let dir = fixture(&[
            ("src/caller.rs", &caller),
            ("src/guard.rs", if same_file_member { "" } else { member }),
            (
                "src/x/h.rs",
                "/// First helper.\npub fn helper(x: i32) {}\n",
            ),
            (
                "src/y/h.rs",
                "/// Second helper.\npub fn helper(x: i32, y: i32) {}\n",
            ),
            (
                "src/nested/other.rs",
                "/// Positive control.\npub fn other() {\n helper(2);\n}\n",
            ),
        ]);
        let mut index = Index::new();
        let mut target = Document::new();
        target.relative_path = "src/x/h.rs".into();
        let symbol = "scip-rust cargo fixture 0.1.0 helper().";
        let mut definition = Occurrence::new();
        definition.range = vec![1, 0, 6];
        definition.symbol = symbol.into();
        definition.symbol_roles = 1;
        target.occurrences.push(definition);
        index.documents.push(target);
        let call_line = caller
            .lines()
            .position(|l| l.contains("helper(2)"))
            .unwrap() as i32;
        for (file, line, column) in [
            ("src/caller.rs", call_line, 9),
            ("src/nested/other.rs", 2, 1),
        ] {
            let mut document = Document::new();
            document.relative_path = file.into();
            let mut reference = Occurrence::new();
            reference.range = vec![line, column, column + 6];
            reference.symbol = symbol.into();
            document.occurrences.push(reference);
            index.documents.push(document);
        }
        fs::write(
            dir.path().join("index.scip"),
            index.write_to_bytes().unwrap(),
        )
        .unwrap();
        let config_path = dir.path().join(".keel/keel.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        config["tier3"]["enabled"] = true.into();
        config["tier3"]["scip_paths"]["rust"] = "index.scip".into();
        fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        let out = keel(dir.path(), &["map", "--verbose"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("tier3 resolved 1 additional references"),
            "same_file_member={same_file_member}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        use keel_core::store::GraphStore;
        use keel_core::types::{EdgeDirection, EdgeKind};
        let store = keel_core::sqlite::SqliteGraphStore::open(
            dir.path().join(".keel/graph.db").to_str().unwrap(),
        )
        .unwrap();
        let wire = store
            .get_nodes_in_file("src/caller.rs")
            .into_iter()
            .find(|n| n.name == "wire")
            .unwrap();
        assert!(!store
            .get_edges(wire.id, EdgeDirection::Outgoing)
            .iter()
            .any(|e| e.kind == EdgeKind::Calls));
        assert_eq!(incoming_calls(dir.path(), "src/x/h.rs", "helper", false), 1);
        assert_eq!(incoming_calls(dir.path(), "src/y/h.rs", "helper", false), 0);
        let member_file = if same_file_member {
            "src/caller.rs"
        } else {
            "src/guard.rs"
        };
        assert_eq!(incoming_calls(dir.path(), member_file, "run", true), 0);
    }
}

#[test]
fn tier3_rejected_self_call_keeps_same_line_helper_resolvable() {
    assert_tier3_self_call_keeps_helper("run(1); helper(2);");
}

#[test]
fn tier3_rejected_nested_self_call_keeps_same_line_helper_resolvable() {
    assert_tier3_self_call_keeps_helper("run(helper(1))");
}

fn assert_tier3_self_call_keeps_helper(body: &str) {
    let caller = format!(
        "use crate::b::run;\n/// Guard.\npub struct Guard;\nimpl Guard {{\n /// Member.\n pub fn run(&self) -> i32 {{\n {body}\n }}\n}}\n"
    );
    let dir = fixture(&[
        ("src/guard.rs", &caller),
        ("src/b.rs", "/// Free.\npub fn run(x: i32) -> i32 { x }\n"),
        (
            "src/x/h.rs",
            "/// First helper.\npub fn helper(x: i32) {}\n",
        ),
        (
            "src/y/h.rs",
            "/// Second helper.\npub fn helper(x: i32, y: i32) {}\n",
        ),
        (
            "src/nested/other.rs",
            "/// Positive control.\npub fn other() {\n helper(2);\n}\n",
        ),
    ]);
    let mut index = Index::new();
    let mut target = Document::new();
    target.relative_path = "src/x/h.rs".into();
    let symbol = "scip-rust cargo fixture 0.1.0 helper().";
    let mut definition = Occurrence::new();
    definition.range = vec![1, 0, 6];
    definition.symbol = symbol.into();
    definition.symbol_roles = 1;
    target.occurrences.push(definition);
    index.documents.push(target);
    let call_line = caller.lines().position(|l| l.contains("helper(")).unwrap() as i32;
    let call_column = caller
        .lines()
        .nth(call_line as usize)
        .unwrap()
        .find("helper")
        .unwrap() as i32;
    for (file, line, column) in [
        ("src/guard.rs", call_line, call_column),
        ("src/nested/other.rs", 2, 1),
    ] {
        let mut document = Document::new();
        document.relative_path = file.into();
        let mut reference = Occurrence::new();
        reference.range = vec![line, column, column + 6];
        reference.symbol = symbol.into();
        document.occurrences.push(reference);
        index.documents.push(document);
    }
    fs::write(
        dir.path().join("index.scip"),
        index.write_to_bytes().unwrap(),
    )
    .unwrap();
    let config_path = dir.path().join(".keel/keel.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["tier3"]["enabled"] = true.into();
    config["tier3"]["scip_paths"]["rust"] = "index.scip".into();
    fs::write(config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let out = keel(dir.path(), &["map", "--verbose"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("tier3 resolved 2 additional references"),
        "body={body}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    use keel_core::store::GraphStore;
    use keel_core::types::{EdgeDirection, EdgeKind};
    let store = keel_core::sqlite::SqliteGraphStore::open(
        dir.path().join(".keel/graph.db").to_str().unwrap(),
    )
    .unwrap();
    let member = store
        .get_nodes_in_file("src/guard.rs")
        .into_iter()
        .find(|n| n.name == "run" && n.is_associated)
        .unwrap();
    let helper = store
        .get_nodes_in_file("src/x/h.rs")
        .into_iter()
        .find(|n| n.name == "helper")
        .unwrap();
    let calls: Vec<_> = store
        .get_edges(member.id, EdgeDirection::Outgoing)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 1, "body={body}: {calls:?}");
    assert_eq!(calls[0].target_id, helper.id);
    assert_eq!(calls[0].line, call_line as u32 + 1);
    assert_eq!(incoming_calls(dir.path(), "src/x/h.rs", "helper", false), 2);
    assert_eq!(incoming_calls(dir.path(), "src/y/h.rs", "helper", false), 0);
    assert_eq!(incoming_calls(dir.path(), "src/guard.rs", "run", true), 0);
    assert_eq!(incoming_calls(dir.path(), "src/b.rs", "run", false), 0);
}

#[test]
fn deleted_free_sibling_at_new_member_line_keeps_first_compile_clean() {
    let prefix = "/// Free.\npub fn run(a: i32) { let _ = a; }\n";
    let rest = "/// Guard.\npub struct G;\nimpl G {\n // Padding 1.\n // Padding 2.\n // Padding 3.\n // Padding 4.\n /// Member.\n pub fn run(&self) {}\n}\n/// Caller.\npub fn wire() {\n run(1);\n}\n";
    let padding = "// Padding.\n".repeat(5);
    let inner = "/// Inner.\npub mod inner {\n pub fn run(a: i32, b: i32) { let _ = (a, b); }\n}\n";
    let source = format!("{prefix}{rest}{padding}{inner}");
    assert_eq!(
        source.lines().nth(10).unwrap().trim(),
        "pub fn run(&self) {}"
    );
    assert!(source
        .lines()
        .nth(23)
        .unwrap()
        .contains("pub fn run(a: i32, b: i32)"));
    let dir = fixture(&[("src/lib.rs", &source)]);
    let edited = format!("{prefix}{}{rest}", "// Shift member.\n".repeat(13));
    assert_eq!(
        edited.lines().nth(23).unwrap().trim(),
        "pub fn run(&self) {}"
    );
    fs::write(dir.path().join("src/lib.rs"), edited).unwrap();
    let out = keel(dir.path(), &["compile", "src/lib.rs", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "first compile: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let result = compile_json(dir.path(), "src/lib.rs");
    assert_no_violation(&result, "E005");
}
