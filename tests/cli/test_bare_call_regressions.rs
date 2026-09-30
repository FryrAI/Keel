//! Round-one counter-cases: preserve base bindings outside the member exclusion.

use std::fs;

use super::test_bare_call_binding::{fixture, incoming_calls};
use crate::common::{assert_no_violation, compile_json, keel, violations_with_code};

const STORE: &str = "/// A store.\npub struct Store;\nimpl Store {\n /// Constructs a store.\n pub fn new(a: i32) -> Self { let _ = a; Store }\n}\n";
const SERVER: &str = "use crate::store::Store;\n/// A server.\npub struct Server;\nimpl Server {\n /// Constructs a server.\n pub fn new(a: i32, b: i32, c: i32) -> Self { let _ = (a,b,c); Server }\n /// Opens a store.\n pub fn open(a: i32) { Store::new(a); }\n}\n";

#[test]
fn cross_file_constructor_keeps_base_target_and_no_false_e005() {
    let dir = fixture(&[("src/store.rs", STORE), ("src/server.rs", SERVER)]);
    assert_eq!(incoming_calls(dir.path(), "src/store.rs", "new", true), 1);
    assert_eq!(incoming_calls(dir.path(), "src/server.rs", "new", true), 0);
    for (file, source) in [("store", STORE), ("server", SERVER)] {
        fs::write(
            dir.path().join(format!("src/{file}.rs")),
            format!("{source}\n// Edited.\n"),
        )
        .unwrap();
    }
    let out = keel(dir.path(), &["compile", "--llm", "--changed"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(out.stdout.is_empty());
    assert_eq!(incoming_calls(dir.path(), "src/store.rs", "new", true), 1);
    assert_eq!(incoming_calls(dir.path(), "src/server.rs", "new", true), 0);
}

#[test]
fn deleting_cross_file_constructor_keeps_base_e004() {
    let dir = fixture(&[("src/store.rs", STORE), ("src/server.rs", SERVER)]);
    fs::write(
        dir.path().join("src/store.rs"),
        "/// A store.\npub struct Store;\n",
    )
    .unwrap();
    let result = compile_json(dir.path(), "src/store.rs");
    assert_eq!(violations_with_code(&result, "E004").len(), 1, "{result}");
}

#[test]
fn two_impl_constructors_keep_base_ambiguity() {
    let source = "/// A.\npub struct A;\nimpl A {\n /// Constructs A.\n pub fn new(a: i32, b: i32, c: i32) -> Self { let _ = (a,b,c); A }\n}\n/// B.\npub struct B;\nimpl B {\n /// Constructs B.\n pub fn new(a: i32) -> Self { let _ = a; B }\n}\n/// Calls B.\npub fn wire() { B::new(1); }\n";
    let dir = fixture(&[("src/lib.rs", source)]);
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "new", true), 0);
    let result = compile_json(dir.path(), "src/lib.rs");
    assert_no_violation(&result, "E005");
    assert_eq!(incoming_calls(dir.path(), "src/lib.rs", "new", true), 0);
}

#[test]
fn external_import_keeps_unfiltered_name_ambiguity() {
    let caller = "use std::fs::read;\n/// A caller.\npub fn wire() { read(\"x\"); }\n";
    let member =
        "/// A store.\npub struct Store;\nimpl Store {\n /// Reads.\n pub fn read(&self) {}\n}\n";
    let free = "/// An unrelated read.\npub fn read(a: i32, b: i32) { let _ = (a,b); }\n";
    let dir = fixture(&[
        ("src/caller.rs", caller),
        ("src/store.rs", member),
        ("src/other.rs", free),
    ]);
    for (file, associated) in [("src/store.rs", true), ("src/other.rs", false)] {
        assert_eq!(incoming_calls(dir.path(), file, "read", associated), 0);
    }
    // Parse the member too: the tier-2 resolver must see the collision.
    fs::write(
        dir.path().join("src/store.rs"),
        format!("{member}\n// Edited.\n"),
    )
    .unwrap();
    fs::write(
        dir.path().join("src/caller.rs"),
        format!("{caller}\n// Edited.\n"),
    )
    .unwrap();
    let out = keel(dir.path(), &["compile", "--llm", "--changed"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(out.stdout.is_empty());
    assert_eq!(incoming_calls(dir.path(), "src/other.rs", "read", false), 0);
}

#[test]
fn sql_exemption_flag_keeps_unqualified_callers() {
    let source = "CREATE FUNCTION my_fn() RETURNS trigger AS $$ BEGIN RETURN NEW; END; $$ LANGUAGE plpgsql;\nCREATE FUNCTION helper_fn(id integer) RETURNS integer AS $$ SELECT id; $$ LANGUAGE sql;\nCREATE TABLE items(id integer);\nCREATE TRIGGER trg BEFORE INSERT ON items FOR EACH ROW EXECUTE FUNCTION my_fn();\nCREATE VIEW values_view AS SELECT helper_fn(id) FROM items;\n";
    let dir = fixture(&[("src/schema.sql", source)]);
    assert_eq!(
        incoming_calls(dir.path(), "src/schema.sql", "my_fn", true),
        1
    );
    assert_eq!(
        incoming_calls(dir.path(), "src/schema.sql", "helper_fn", true),
        2
    );
}

#[test]
fn deleted_same_file_target_does_not_roll_back_other_edges() {
    for (extension, source, edited) in [
        ("rs", "/// Helper.\npub fn helper() {}\n/// Other.\npub fn other() {}\n/// A.\npub fn a() { helper(); }\n/// B.\npub fn b() {}\n", "/// Other.\npub fn other() {}\n/// A.\npub fn a() { helper(); }\n/// B.\npub fn b() { other(); }\n"),
        ("py", "def helper() -> None:\n    \"\"\"Helper.\"\"\"\n    pass\ndef other() -> None:\n    \"\"\"Other.\"\"\"\n    pass\ndef a() -> None:\n    \"\"\"A.\"\"\"\n    helper()\ndef b() -> None:\n    \"\"\"B.\"\"\"\n    pass\n", "def other() -> None:\n    \"\"\"Other.\"\"\"\n    pass\ndef a() -> None:\n    \"\"\"A.\"\"\"\n    helper()\ndef b() -> None:\n    \"\"\"B.\"\"\"\n    other()\n"),
    ] {
        let file = format!("src/lib.{extension}");
        let dir = fixture(&[(&file, source)]);
        fs::write(dir.path().join(&file), edited).unwrap();
        let out = keel(dir.path(), &["compile", "--verbose", &file]);
        assert!(!String::from_utf8_lossy(&out.stderr).contains("graph sync (edges) failed"), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(incoming_calls(dir.path(), &file, "other", false), 1);
    }
}

#[test]
fn nested_module_duplicate_names_keep_base_last_local_binding() {
    let source =
        "mod a {\n pub fn f(x: i32) {}\n}\nmod b {\n pub fn f() {}\n pub fn caller() { f(); }\n}\n";
    let dir = fixture(&[("src/lib.rs", source)]);
    let store = keel_core::sqlite::SqliteGraphStore::open(
        dir.path().join(".keel/graph.db").to_str().unwrap(),
    )
    .unwrap();
    use keel_core::store::GraphStore;
    use keel_core::types::{EdgeDirection, EdgeKind};
    let nodes = store.get_nodes_in_file("src/lib.rs");
    for node in nodes.iter().filter(|n| n.name == "f") {
        let calls = store
            .get_edges(node.id, EdgeDirection::Incoming)
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count();
        assert_eq!(calls, usize::from(node.line_start == 5));
    }
}

#[test]
fn static_block_local_function_keeps_map_and_compile_binding() {
    for extension in ["js", "ts"] {
        let file = format!("src/class.{extension}");
        let source =
            "class C {\n static {\n  function helper(x) { return x; }\n  helper(1);\n }\n}\n";
        let dir = fixture(&[(&file, source)]);
        assert_eq!(incoming_calls(dir.path(), &file, "helper", false), 1);
        let result = compile_json(dir.path(), &file);
        assert_no_violation(&result, "E005");
        assert_eq!(incoming_calls(dir.path(), &file, "helper", false), 1);
        fs::write(
            dir.path().join(&file),
            source.replace("helper(1)", "helper(1, 2)"),
        )
        .unwrap();
        let result = compile_json(dir.path(), &file);
        assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
        assert_eq!(incoming_calls(dir.path(), &file, "helper", false), 1);
    }
}

#[test]
fn python_and_typescript_free_function_wins_over_member_in_either_order() {
    for (extension, free, member, caller) in [
        ("py", "def run(a: int) -> None:\n    \"\"\"Free.\"\"\"\n    pass\n", "class Guard:\n    \"\"\"Guard.\"\"\"\n    def run(self) -> None:\n        \"\"\"Member.\"\"\"\n        pass\n", "def wire() -> None:\n    \"\"\"Caller.\"\"\"\n    run(1)\n"),
        ("ts", "/** Free. */\nfunction run(a: number): void {}\n", "/** Guard. */\nclass Guard {\n /** Member. */\n run(): void {}\n}\n", "/** Caller. */\nfunction wire(): void { run(1); }\n"),
    ] {
        for (first, second) in [(free, member), (member, free)] {
            let file = format!("src/lib.{extension}");
            let source = format!("{first}{second}{caller}");
            let dir = fixture(&[(&file, &source)]);
            assert_eq!(incoming_calls(dir.path(), &file, "run", true), 0);
            assert_eq!(incoming_calls(dir.path(), &file, "run", false), 1);
            fs::write(dir.path().join(&file), source.replace("run(1)", "run(1, 2)")).unwrap();
            let result = compile_json(dir.path(), &file);
            assert_eq!(violations_with_code(&result, "E005").len(), 1, "{result}");
            assert_eq!(incoming_calls(dir.path(), &file, "run", true), 0);
            assert_eq!(incoming_calls(dir.path(), &file, "run", false), 1);
        }
    }
}
