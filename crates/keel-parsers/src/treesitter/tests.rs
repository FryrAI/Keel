use std::path::Path;

use super::*;
use crate::resolver::ReferenceKind;
use keel_core::types::NodeKind;

#[test]
fn test_parse_typescript_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("test.ts"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn test_typescript_return_type_has_no_leading_colon() {
    // The TS type-annotation node includes the leading `:`; the signature must
    // render `add(...) -> number`, never `-> : number` (leaks into E001
    // fix_hints and discover output).
    let mut parser = TreeSitterParser::new();
    let source = "function add(a: number, b: number): number {\n  return a + b;\n}\n";
    let result = parser
        .parse_file("typescript", Path::new("math.ts"), source)
        .unwrap();
    let add = result
        .definitions
        .iter()
        .find(|d| d.name == "add")
        .expect("add function");
    assert!(
        add.signature.contains("-> number"),
        "signature should render a clean return type, got {:?}",
        add.signature
    );
    assert!(
        !add.signature.contains("-> :") && !add.signature.contains(": number ->"),
        "return type must not carry a leading colon, got {:?}",
        add.signature
    );
}

#[test]
fn test_parse_python_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
def greet(name: str) -> str:
    return f"Hello, {name}!"
"#;
    let result = parser
        .parse_file("python", Path::new("test.py"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn test_parse_go_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
package main

func greet(name string) string {
    return "Hello, " + name
}
"#;
    let result = parser
        .parse_file("go", Path::new("test.go"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn test_parse_rust_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
    let result = parser
        .parse_file("rust", Path::new("test.rs"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn test_parse_typescript_class() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
class UserService {
    getUser(id: number): User {
        return this.db.find(id);
    }
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("service.ts"), source)
        .unwrap();
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "UserService");
}

#[test]
fn test_parse_typescript_imports() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
import { foo, bar } from './utils';
import axios from 'axios';
"#;
    let result = parser
        .parse_file("typescript", Path::new("app.ts"), source)
        .unwrap();
    assert!(result.imports.len() >= 2);
    let relative: Vec<_> = result.imports.iter().filter(|i| i.is_relative).collect();
    assert!(!relative.is_empty());
}

#[test]
fn test_parse_typescript_calls() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
function main() {
    const result = greet("world");
    console.log(result);
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("main.ts"), source)
        .unwrap();
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(calls.len() >= 2);
}

#[test]
fn test_detect_language() {
    assert_eq!(detect_language(Path::new("foo.ts")), Some("typescript"));
    assert_eq!(detect_language(Path::new("bar.py")), Some("python"));
    assert_eq!(detect_language(Path::new("baz.go")), Some("go"));
    assert_eq!(detect_language(Path::new("qux.rs")), Some("rust"));
    assert_eq!(detect_language(Path::new("readme.md")), None);
}

#[test]
fn test_unsupported_language() {
    let mut parser = TreeSitterParser::new();
    let result = parser.parse_file("haskell", Path::new("test.hs"), "main = putStrLn");
    assert!(result.is_err());
}

#[test]
fn test_python_decorated_function_no_duplicate() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
@app.route("/data")
def get_data():
    return {"ok": True}

def plain_func():
    pass
"#;
    let result = parser
        .parse_file("python", Path::new("views.py"), source)
        .unwrap();
    // Filter out auto-created Module node — only count functions
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(
        funcs.len(),
        2,
        "decorated function should not produce a duplicate: {:?}",
        funcs.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    let get_data = funcs.iter().find(|d| d.name == "get_data").unwrap();
    // line_start should be the `def` line (3), not the decorator line (2)
    assert_eq!(
        get_data.line_start, 3,
        "line_start should be the def line, not the decorator"
    );
}

#[test]
fn test_rust_docstring_extraction() {
    let mut parser = TreeSitterParser::new();
    let source = "/// This is a doc comment.\npub fn foo() -> i32 {\n    42\n}\n";
    let result = parser
        .parse_file("rust", Path::new("test.rs"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "foo");
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("This is a doc comment."),
        "docstring should be extracted from /// comment"
    );
}

#[test]
fn test_rust_docstring_with_attribute() {
    let mut parser = TreeSitterParser::new();
    let source = "/// Doc before attr.\n#[allow(dead_code)]\npub fn bar() {}\n";
    let result = parser
        .parse_file("rust", Path::new("test.rs"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("Doc before attr."),
        "docstring should be found even with attribute between doc and fn"
    );
}

#[test]
fn test_rust_method_docstring_in_impl() {
    let mut parser = TreeSitterParser::new();
    let source = "struct Foo;\n\nimpl Foo {\n    /// Method doc.\n    pub fn do_thing(&self) -> bool {\n        true\n    }\n}\n";
    let result = parser
        .parse_file("rust", Path::new("test.rs"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function && d.name == "do_thing")
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("Method doc."),
        "docstring on method inside impl block should be extracted"
    );
}

#[test]
fn test_no_docstring() {
    let mut parser = TreeSitterParser::new();
    let source = "pub fn no_doc() {}\n";
    let result = parser
        .parse_file("rust", Path::new("test.rs"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert!(
        funcs[0].docstring.is_none(),
        "function without doc comment should have None docstring"
    );
}

#[test]
fn test_python_docstring_extraction() {
    let mut parser = TreeSitterParser::new();
    let source = "def greet(name: str) -> str:\n    \"\"\"Greet someone.\"\"\"\n    return f\"Hello, {name}!\"\n";
    let result = parser
        .parse_file("python", Path::new("test.py"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("Greet someone."),
        "Python triple-quoted docstring should be extracted"
    );
}

#[test]
fn test_typescript_jsdoc_extraction() {
    let mut parser = TreeSitterParser::new();
    let source = "/** Greets a user. */\nfunction greet(name: string): string {\n    return `Hello, ${name}!`;\n}\n";
    let result = parser
        .parse_file("typescript", Path::new("test.ts"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("Greets a user."),
        "JSDoc comment should be extracted"
    );
}

#[test]
fn test_typescript_exported_function_jsdoc() {
    let mut parser = TreeSitterParser::new();
    let source = "/** Activates the extension. */\nexport function activate(ctx: ExtensionContext) {\n    console.log('active');\n}\n";
    let result = parser
        .parse_file("typescript", Path::new("ext.ts"), source)
        .unwrap();
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("Activates the extension."),
        "JSDoc on exported function should be extracted via parent export_statement"
    );
}

#[test]
fn test_python_decorated_class_no_duplicate() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
@dataclass
class Config:
    host: str
    port: int
"#;
    let result = parser
        .parse_file("python", Path::new("models.py"), source)
        .unwrap();
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(
        classes.len(),
        1,
        "decorated class should not produce a duplicate"
    );
    assert_eq!(
        classes[0].line_start, 3,
        "line_start should be the class line, not the decorator"
    );
}
// --- Rust test-context marking (issue #38) ---

#[test]
fn rust_marks_definitions_in_cfg_test_module() {
    let mut parser = TreeSitterParser::new();
    let source = "\
pub fn production() {}\n\
fn dead_helper() {}\n\
\n\
#[cfg(test)]\n\
mod tests {\n\
    fn helper_no_prefix() {}\n\
\n\
    #[test]\n\
    fn terminal_never_retries() {}\n\
\n\
    #[tokio::test]\n\
    async fn async_case() {}\n\
}\n";
    let result = parser
        .parse_file("rust", Path::new("src/thing.rs"), source)
        .unwrap();
    let ctx = |name: &str| {
        result
            .definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("missing def {name}"))
            .in_test_context
    };

    // Inside `#[cfg(test)] mod tests` — all exempt regardless of naming.
    assert!(
        ctx("helper_no_prefix"),
        "cfg(test) module fn is test-context"
    );
    assert!(ctx("terminal_never_retries"), "#[test] fn is test-context");
    assert!(ctx("async_case"), "#[tokio::test] fn is test-context");

    // Production code is NOT test-context.
    assert!(!ctx("production"), "top-level pub fn is not test-context");
    assert!(
        !ctx("dead_helper"),
        "top-level private fn is not test-context"
    );
}

#[test]
fn non_rust_definitions_are_never_test_context() {
    let mut parser = TreeSitterParser::new();
    let result = parser
        .parse_file("typescript", Path::new("a.ts"), "function f() {}\n")
        .unwrap();
    assert!(result.definitions.iter().all(|d| !d.in_test_context));
}

#[test]
fn rust_trait_context_covers_decls_and_trait_impls_only() {
    let mut parser = TreeSitterParser::new();
    let source = "\
trait Store {\n\
    fn required(&self) -> bool;\n\
    fn defaulted(&self) -> bool { true }\n\
}\n\
struct S;\n\
impl Store for S {\n\
    fn required(&self) -> bool { false }\n\
}\n\
impl S {\n\
    fn inherent(&self) -> bool { true }\n\
}\n\
fn free_fn() -> bool { true }\n";
    let result = parser
        .parse_file("rust", Path::new("src/store.rs"), source)
        .unwrap();
    let ctx = |name: &str| {
        result
            .definitions
            .iter()
            .filter(|d| d.name == name)
            .map(|d| d.in_trait_context)
            .collect::<Vec<_>>()
    };

    // A DEFAULTED trait method (it has a body) is extracted as a definition
    // and is trait-context. This is the case that mattered in practice:
    // `GraphStore::find_body_matches` and friends were being reported as dead
    // code and as duplicate names before this flag existed.
    assert_eq!(ctx("defaulted"), vec![true], "defaulted trait method");
    // A bodyless trait declaration (`fn required(&self) -> bool;`) is not
    // extracted as a definition at all — only the `impl Store for S` copy is,
    // and that one is trait-context.
    assert_eq!(
        ctx("required"),
        vec![true],
        "only the trait impl is a definition, and it is trait-context"
    );
    // An inherent `impl S` block is NOT a trait context.
    assert_eq!(ctx("inherent"), vec![false], "inherent method");
    assert_eq!(ctx("free_fn"), vec![false], "free function");
}

#[test]
fn ts_interface_context_requires_implements_or_interface() {
    let mut parser = TreeSitterParser::new();
    let source = "\
class Provider implements vscode.HoverProvider {\n\
    provideHover() { return undefined; }\n\
}\n\
class Plain {\n\
    ordinary() { return 1; }\n\
}\n\
class Derived extends Base {\n\
    inherited() { return 2; }\n\
}\n";
    let result = parser
        .parse_file("typescript", Path::new("a.ts"), source)
        .unwrap();
    let ctx = |name: &str| {
        result
            .definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("missing def {name}"))
            .in_trait_context
    };

    assert!(
        ctx("provideHover"),
        "`implements` body is interface-context"
    );
    assert!(!ctx("ordinary"), "plain class is not interface-context");
    assert!(
        !ctx("inherited"),
        "`extends` alone is not interface-context"
    );
}

#[test]
fn go_and_python_are_never_trait_context() {
    let mut parser = TreeSitterParser::new();
    let go = parser
        .parse_file("go", Path::new("a.go"), "func Handle() {}\n")
        .unwrap();
    assert!(go.definitions.iter().all(|d| !d.in_trait_context));
    let py = parser
        .parse_file("python", Path::new("a.py"), "def handle():\n    pass\n")
        .unwrap();
    assert!(py.definitions.iter().all(|d| !d.in_trait_context));
}

#[test]
fn functions_named_as_values_are_captured_as_value_references() {
    let mut parser = TreeSitterParser::new();
    let source = "\
fn wire() {\n\
    let a: Vec<String> = xs.iter().map(render_file).collect();\n\
    let r = Router::new().route(\"/health\", get(health));\n\
}\n\
struct C {\n\
    #[serde(default = \"default_true\")]\n\
    flag: bool,\n\
}\n";
    let result = parser
        .parse_file("rust", Path::new("src/wire.rs"), source)
        .unwrap();
    let value_ref = |name: &str| {
        result
            .references
            .iter()
            .any(|r| r.name == name && r.kind == ReferenceKind::Value)
    };

    // Passed as an argument, never invoked — a real usage, not a call.
    assert!(value_ref("render_file"), "`.map(render_file)`");
    assert!(value_ref("health"), "`get(health)`");
    // Named in a serde attribute string.
    assert!(value_ref("default_true"), "#[serde(default = \"...\")]");

    // Crucially NOT calls: a value reference must never build a `calls` edge
    // or feed E005 arity checking (it has no argument list to count).
    assert!(
        !result
            .references
            .iter()
            .any(|r| r.name == "render_file" && r.kind == ReferenceKind::Call),
        "function reference must not be recorded as a call"
    );
}

/// Associated items (inherent-impl fns, class methods, Go receiver funcs) are
/// marked `is_associated`; free functions are not. W002 relies on this to skip
/// idiomatic `Type::name` bare-name collisions across unrelated types.
#[test]
fn associated_items_are_marked_across_languages() {
    let mut parser = TreeSitterParser::new();

    let rust = "struct S;\nimpl S { pub fn in_memory() -> Self { S } }\npub fn free_fn() {}\n";
    let result = parser
        .parse_file("rust", std::path::Path::new("a.rs"), rust)
        .unwrap();
    let assoc = result
        .definitions
        .iter()
        .find(|d| d.name == "in_memory")
        .unwrap();
    let free = result
        .definitions
        .iter()
        .find(|d| d.name == "free_fn")
        .unwrap();
    assert!(assoc.is_associated, "inherent-impl fn is associated");
    assert!(!free.is_associated, "free fn is not associated");

    let go = "package p\n\ntype T struct{}\n\nfunc (t T) IsExpired() bool { return false }\n\nfunc Free() {}\n";
    let result = parser
        .parse_file("go", std::path::Path::new("a.go"), go)
        .unwrap();
    let assoc = result
        .definitions
        .iter()
        .find(|d| d.name == "IsExpired")
        .unwrap();
    let free = result
        .definitions
        .iter()
        .find(|d| d.name == "Free")
        .unwrap();
    assert!(assoc.is_associated, "Go receiver func is associated");
    assert!(!free.is_associated, "Go free func is not associated");

    let py = "class C:\n    def is_expired(self) -> bool:\n        return False\n\n\ndef free_fn() -> None:\n    pass\n";
    let result = parser
        .parse_file("python", std::path::Path::new("a.py"), py)
        .unwrap();
    let assoc = result
        .definitions
        .iter()
        .find(|d| d.name == "is_expired")
        .unwrap();
    let free = result
        .definitions
        .iter()
        .find(|d| d.name == "free_fn")
        .unwrap();
    assert!(assoc.is_associated, "Python class method is associated");
    assert!(!free.is_associated, "Python free fn is not associated");
}

/// A local `fn` nested INSIDE an impl method is a private helper, not an
/// associated item — it is not reachable as `Type::name` and carries no
/// contract. The associated-item walk previously ran all the way to the root,
/// so it saw the enclosing `impl_item` and marked the helper associated, which
/// made W002 silently skip genuine duplicate-name warnings on local helpers.
#[test]
fn nested_local_fn_inside_impl_method_is_not_associated() {
    let mut parser = TreeSitterParser::new();
    let rust = "struct S;\n\
                impl S {\n\
                \x20   pub fn outer(&self) -> u32 {\n\
                \x20       fn helper(x: u32) -> u32 { x + 1 }\n\
                \x20       helper(1)\n\
                \x20   }\n\
                }\n";
    let result = parser
        .parse_file("rust", std::path::Path::new("a.rs"), rust)
        .unwrap();

    let outer = result
        .definitions
        .iter()
        .find(|d| d.name == "outer")
        .expect("the impl method is extracted");
    let helper = result
        .definitions
        .iter()
        .find(|d| d.name == "helper")
        .expect("the nested local fn is extracted");

    assert!(
        outer.is_associated,
        "the impl method itself IS an associated item"
    );
    assert!(
        !helper.is_associated,
        "a local fn nested inside an impl method is NOT an associated item"
    );
}

/// The asymmetry is deliberate: the test-context flag is NOT bounded by an
/// enclosing function scope. A helper nested inside a `#[test]` fn is still
/// test code, and E002/E003/W005 must keep skipping it.
#[test]
fn nested_fn_inside_test_fn_is_still_test_context() {
    let mut parser = TreeSitterParser::new();
    let rust = "#[cfg(test)]\n\
                mod tests {\n\
                \x20   #[test]\n\
                \x20   fn outer_test() {\n\
                \x20       fn helper(x: u32) -> u32 { x + 1 }\n\
                \x20       assert_eq!(helper(1), 2);\n\
                \x20   }\n\
                }\n";
    let result = parser
        .parse_file("rust", std::path::Path::new("a.rs"), rust)
        .unwrap();

    let helper = result
        .definitions
        .iter()
        .find(|d| d.name == "helper")
        .expect("the nested local fn is extracted");
    assert!(
        helper.in_test_context,
        "a fn nested inside a #[test] fn is still test code"
    );
}
