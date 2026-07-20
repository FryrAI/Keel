use std::path::Path;

use super::*;

#[test]
fn test_go_resolver_parse_function() {
    let resolver = GoResolver::new();
    let source = r#"
package main

func Greet(name string) string {
    return "Hello, " + name
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "Greet");
    assert!(funcs[0].is_public);
}

#[test]
fn test_go_resolver_private_function() {
    let resolver = GoResolver::new();
    let source = r#"
package main

func greet(name string) string {
    return "Hello, " + name
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert!(!funcs[0].is_public);
}

#[test]
fn test_go_resolver_caches_results() {
    let resolver = GoResolver::new();
    let source = "package main\nfunc Hello() {}";
    let path = Path::new("cached.go");
    resolver.parse_file(path, source);
    let defs = resolver.resolve_definitions(path);
    let funcs: Vec<_> = defs
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
}

#[test]
fn test_go_resolver_same_file_call_edge() {
    let resolver = GoResolver::new();
    let source = r#"
package main

func helper() int { return 1 }
func main() { helper() }
"#;
    let path = Path::new("edge.go");
    resolver.parse_file(path, source);
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "edge.go".into(),
        line: 5,
        callee_name: "helper".into(),
        receiver: None,
    });
    assert!(edge.is_some());
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert!(edge.confidence >= 0.90);
}

#[test]
fn test_go_package_alias() {
    assert_eq!(go_package_alias("\"fmt\""), "fmt");
    assert_eq!(go_package_alias("\"net/http\""), "http");
    assert_eq!(go_package_alias("\"github.com/user/repo/pkg\""), "pkg");
}

#[test]
fn test_go_import_extracts_package_name() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import (
    "fmt"
    "github.com/spf13/cobra"
)

func main() {
    fmt.Println("hello")
    cobra.Execute()
}
"#;
    let path = Path::new("test_imports.go");
    let result = resolver.parse_file(path, source);
    let cobra_imp = result.imports.iter().find(|i| i.source.contains("cobra"));
    assert!(cobra_imp.is_some(), "should have cobra import");
    let imp = cobra_imp.unwrap();
    assert!(
        imp.imported_names.contains(&"cobra".to_string()),
        "imported_names should contain 'cobra', got: {:?}",
        imp.imported_names
    );
}

#[test]
fn test_go_test_and_benchmark_are_test_context() {
    let resolver = GoResolver::new();
    let source = r#"
package widget

import "testing"

func TestFoo(t *testing.T) {}

func BenchmarkFoo(b *testing.B) {}

func Testing(x int) string { return "" }

func regular() {}
"#;
    let result = resolver.parse_file(Path::new("widget_test.go"), source);
    let flag = |name: &str| {
        result
            .definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("missing def {name}"))
            .in_test_context
    };
    assert!(flag("TestFoo"), "TestFoo(t *testing.T) is test context");
    assert!(
        flag("BenchmarkFoo"),
        "BenchmarkFoo(b *testing.B) is test context"
    );
    // Name starts with `Test` but has no *testing.T parameter — must NOT be
    // treated as a test, proving the check is not a bare name-prefix guess.
    assert!(
        !flag("Testing"),
        "Testing(x int) is not a test despite the Test prefix"
    );
    assert!(!flag("regular"), "an ordinary function is not test context");
}

#[test]
fn test_go_entrypoints_are_auto_invoked() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import "testing"

func init() {}

func main() {}

func TestMain(m *testing.M) {}

func helper() {}
"#;
    let result = resolver.parse_file(Path::new("main.go"), source);
    let def = |name: &str| {
        result
            .definitions
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("missing def {name}"))
    };
    assert!(def("init").is_auto_invoked, "init is auto-invoked");
    assert!(def("main").is_auto_invoked, "main is auto-invoked");
    assert!(def("TestMain").is_auto_invoked, "TestMain is auto-invoked");
    // TestMain takes *testing.M, not *testing.T — auto-invoked, not test-context.
    assert!(
        !def("TestMain").in_test_context,
        "TestMain is an entrypoint, not itself a test"
    );
    assert!(!def("helper").is_auto_invoked, "helper is not auto-invoked");
}

#[test]
fn test_go_cross_file_call_with_import() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import "github.com/spf13/cobra"

func main() {
    cobra.Execute()
}
"#;
    let path = Path::new("test_cross.go");
    let result = resolver.parse_file(path, source);
    assert!(!result.imports.is_empty(), "should have imports");
    let imp = &result.imports[0];
    assert!(
        imp.source.contains("cobra"),
        "import source should contain cobra"
    );
}
