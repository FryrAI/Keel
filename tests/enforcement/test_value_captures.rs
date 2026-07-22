//! W005 false-positive regression tests for function-as-value references.
//!
//! keel suppresses `W005 dead_code` for any function whose name appears as a
//! reference in the compile batch. Function-as-value usages — a callback passed
//! by keyword, an entry in a dispatch table, a returned closure, an alias
//! binding — are real usage, but tree-sitter query child patterns match DIRECT
//! children only, so before the nested value-position patterns landed in the
//! `.scm` files the identifier was invisible and the function read as dead.
//!
//! Each test drives the real binary (init -> map -> compile --json) over a
//! fixture that mixes registered/callback functions with one genuinely unused
//! control function: the control MUST fire W005 (proving the check is live and
//! the fixture is not exempt), the registered ones must NOT.

use std::path::Path;

use crate::common::{compile_json, mapped_project, violations_with_code};

/// Names of every function reported dead by `keel compile <file> --json`.
/// A clean compile prints nothing, so empty stdout means "no violations".
fn dead_function_names(dir: &Path, file: &str) -> Vec<String> {
    let result = compile_json(dir, file);
    violations_with_code(&result, "W005")
        .iter()
        .filter_map(|v| {
            // "Function `name` has no callers"
            v["message"].as_str()?.split('`').nth(1).map(str::to_string)
        })
        .collect()
}

/// Assert the control function is dead and every value-referenced function is
/// not, in one place so a failure prints the whole W005 set.
fn assert_only_control_is_dead(dead: &[String], control: &str, referenced: &[&str]) {
    assert!(
        dead.iter().any(|n| n == control),
        "control `{control}` must fire W005 (otherwise the fixture is exempt \
         and the test proves nothing); W005 fired for: {dead:?}"
    );
    for name in referenced {
        assert!(
            !dead.iter().any(|n| n == name),
            "`{name}` is used as a value — W005 must not fire; W005 fired for: {dead:?}"
        );
    }
}

/// Python: `key=` callbacks, dict dispatch tables, returned functions, bare
/// decorators and alias assignments. `__all__` makes everything else private,
/// which is what puts these functions in W005's scope at all.
#[test]
fn python_value_referenced_functions_are_not_dead() {
    let dir = mapped_project(&[(
        "src/wire.py",
        "__all__ = [\"run\"]\n\
         \n\
         \n\
         def sort_key_target(item: int) -> int:\n\
         \x20   \"\"\"Sort key passed by keyword.\"\"\"\n\
         \x20   return item\n\
         \n\
         \n\
         def dict_dispatch_target(payload: int) -> int:\n\
         \x20   \"\"\"Handler stored in a dispatch table.\"\"\"\n\
         \x20   return payload\n\
         \n\
         \n\
         def list_target(payload: int) -> int:\n\
         \x20   \"\"\"Handler stored in a list.\"\"\"\n\
         \x20   return payload\n\
         \n\
         \n\
         def return_target(payload: int) -> int:\n\
         \x20   \"\"\"Returned by a factory.\"\"\"\n\
         \x20   return payload\n\
         \n\
         \n\
         def alias_target(payload: int) -> int:\n\
         \x20   \"\"\"Bound to an alias.\"\"\"\n\
         \x20   return payload\n\
         \n\
         \n\
         def decorator_target(fn: object) -> object:\n\
         \x20   \"\"\"Used as a bare decorator.\"\"\"\n\
         \x20   return fn\n\
         \n\
         \n\
         def dead_control(payload: int) -> int:\n\
         \x20   \"\"\"Nothing references this one.\"\"\"\n\
         \x20   return payload\n\
         \n\
         \n\
         HANDLERS = {\"evt\": dict_dispatch_target}\n\
         STEPS = [list_target]\n\
         ALIAS = alias_target\n\
         \n\
         \n\
         @decorator_target\n\
         def registered() -> int:\n\
         \x20   \"\"\"Registered through a bare decorator.\"\"\"\n\
         \x20   return 1\n\
         \n\
         \n\
         def make_handler() -> object:\n\
         \x20   \"\"\"Return the handler itself.\"\"\"\n\
         \x20   return return_target\n\
         \n\
         \n\
         def run(items: list) -> list:\n\
         \x20   \"\"\"Public entry point.\"\"\"\n\
         \x20   return sorted(items, key=sort_key_target)\n",
    )]);

    let dead = dead_function_names(dir.path(), "src/wire.py");
    assert_only_control_is_dead(
        &dead,
        "dead_control",
        &[
            "sort_key_target",
            "dict_dispatch_target",
            "list_target",
            "return_target",
            "alias_target",
            "decorator_target",
        ],
    );
}

/// TypeScript: object-literal dispatch tables, shorthand properties, array
/// elements, returned functions and alias declarations.
#[test]
fn typescript_value_referenced_functions_are_not_dead() {
    let dir = mapped_project(&[(
        "src/wire.ts",
        "function tableHandler(x: number): number {\n  return x + 1;\n}\n\
         \n\
         function stringLookupTarget(x: number): number {\n  return x + 2;\n}\n\
         \n\
         function arrayTarget(x: number): number {\n  return x + 3;\n}\n\
         \n\
         function returnTarget(x: number): number {\n  return x + 4;\n}\n\
         \n\
         function aliasTarget(x: number): number {\n  return x + 5;\n}\n\
         \n\
         function deadControl(x: number): number {\n  return x + 6;\n}\n\
         \n\
         const table = { evt: tableHandler };\n\
         const shorthand = { stringLookupTarget };\n\
         const steps = [arrayTarget];\n\
         const alias = aliasTarget;\n\
         \n\
         export function pick(): unknown {\n\
         \x20 return returnTarget;\n\
         }\n\
         \n\
         export function wire(): unknown[] {\n\
         \x20 return [table, shorthand, steps, alias];\n\
         }\n",
    )]);

    let dead = dead_function_names(dir.path(), "src/wire.ts");
    assert_only_control_is_dead(
        &dead,
        "deadControl",
        &[
            "tableHandler",
            "stringLookupTarget",
            "arrayTarget",
            "returnTarget",
            "aliasTarget",
        ],
    );
}

/// Go: composite-literal job tables (bare and keyed elements), returned
/// functions and `:=` / `var` bindings. Lowercase names are package-private,
/// which is what puts them in W005's scope.
#[test]
fn go_value_referenced_functions_are_not_dead() {
    let dir = mapped_project(&[(
        "src/wire.go",
        "package wire\n\
         \n\
         func tableJob() int { return 1 }\n\
         \n\
         func keyedJob() int { return 2 }\n\
         \n\
         func returnTarget() int { return 3 }\n\
         \n\
         func shortTarget() int { return 4 }\n\
         \n\
         func varTarget() int { return 5 }\n\
         \n\
         func deadControl() int { return 6 }\n\
         \n\
         var jobs = []func() int{tableJob}\n\
         \n\
         var routes = map[string]func() int{\"evt\": keyedJob}\n\
         \n\
         var aliased = varTarget\n\
         \n\
         // Pick returns the handler itself.\n\
         func Pick() func() int {\n\
         \treturn returnTarget\n\
         }\n\
         \n\
         // Wire binds the remaining handlers.\n\
         func Wire() []func() int {\n\
         \tshort := shortTarget\n\
         \treturn []func() int{short, aliased, jobs[0], routes[\"evt\"]}\n\
         }\n",
    )]);

    let dead = dead_function_names(dir.path(), "src/wire.go");
    assert_only_control_is_dead(
        &dead,
        "deadControl",
        &[
            "tableJob",
            "keyedJob",
            "returnTarget",
            "shortTarget",
            "varTarget",
        ],
    );
}

/// Rust: struct-literal field values, `&ident` arguments, `let` aliases, array
/// elements and returned functions.
#[test]
fn rust_value_referenced_functions_are_not_dead() {
    let dir = mapped_project(&[(
        "src/wire.rs",
        "struct Holder {\n\
         \x20   cb: fn() -> u32,\n\
         }\n\
         \n\
         fn struct_field_target() -> u32 {\n\
         \x20   1\n\
         }\n\
         \n\
         fn borrowed_target() -> u32 {\n\
         \x20   2\n\
         }\n\
         \n\
         fn let_target() -> u32 {\n\
         \x20   3\n\
         }\n\
         \n\
         fn array_target() -> u32 {\n\
         \x20   4\n\
         }\n\
         \n\
         fn return_target() -> u32 {\n\
         \x20   5\n\
         }\n\
         \n\
         fn dead_control() -> u32 {\n\
         \x20   6\n\
         }\n\
         \n\
         fn register(_cb: &fn() -> u32) {}\n\
         \n\
         /// Build the holder and register the callbacks.\n\
         pub fn wire() -> Holder {\n\
         \x20   register(&borrowed_target);\n\
         \x20   let alias = let_target;\n\
         \x20   let table = [array_target];\n\
         \x20   let _ = (alias, table);\n\
         \x20   Holder {\n\
         \x20       cb: struct_field_target,\n\
         \x20   }\n\
         }\n\
         \n\
         /// Hand back the returned callback.\n\
         pub fn pick() -> fn() -> u32 {\n\
         \x20   return return_target;\n\
         }\n",
    )]);

    let dead = dead_function_names(dir.path(), "src/wire.rs");
    assert_only_control_is_dead(
        &dead,
        "dead_control",
        &[
            "struct_field_target",
            "borrowed_target",
            "let_target",
            "array_target",
            "return_target",
        ],
    );
}
