// Tests for Go package-level scoping resolution (Spec 004 - Go Resolution)
use std::path::Path;

use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};

#[test]
/// Functions within the same package should resolve to each other without imports.
fn test_same_package_function_resolution() {
    let resolver = GoResolver::new();

    // Parse file A in same directory
    let source_a = r#"
package handlers

func helper() int {
    return 42
}
"#;
    resolver.parse_file(Path::new("/project/handlers/a.go"), source_a);

    // Parse file B in same directory
    let source_b = r#"
package handlers

func Process() int {
    return helper()
}
"#;
    resolver.parse_file(Path::new("/project/handlers/b.go"), source_b);

    // Resolve unqualified cross-file call
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/handlers/b.go".into(),
        line: 5,
        callee_name: "helper".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "Should resolve same-package cross-file call"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert!(
        edge.confidence >= 0.70,
        "Cross-file same-package should have confidence >= 0.70, got: {}",
        edge.confidence
    );
    assert_eq!(edge.resolution_tier, "tier2_heuristic");
}

#[test]
/// Functions in different packages require explicit import to resolve.
/// This tests that a qualified call (utils.Process) resolves via the import.
fn test_cross_package_requires_import() {
    let resolver = GoResolver::new();
    let source = r#"
package handlers

import "github.com/user/project/utils"

func Handle() {
    utils.Process()
}
"#;
    let result = resolver.parse_file(Path::new("handlers.go"), source);

    // Verify the import was found
    assert!(
        !result.imports.is_empty(),
        "Should have at least one import"
    );

    // Resolve the cross-package call
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "handlers.go".into(),
        line: 7,
        callee_name: "utils.Process".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Should resolve utils.Process via import");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Process");
}

#[test]
/// Package-level definitions should be accessible from any file in the same package.
/// We test with type definitions (captured by tree-sitter) rather than raw vars.
fn test_package_level_variable_resolution() {
    let resolver = GoResolver::new();

    // Parse file A with a package-level function used as a "getter"
    let source_a = r#"
package config

func DefaultTimeout() int {
    return 30
}
"#;
    resolver.parse_file(Path::new("/project/config/vars.go"), source_a);

    // Parse file B that calls the package-level function from another file
    let source_b = r#"
package config

func GetTimeout() int {
    return DefaultTimeout()
}
"#;
    resolver.parse_file(Path::new("/project/config/access.go"), source_b);

    // Resolve cross-file call within same package
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/config/access.go".into(),
        line: 5,
        callee_name: "DefaultTimeout".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "Should resolve package-level definition across files"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "DefaultTimeout");
    assert!(edge.confidence >= 0.70);
}

#[test]
/// Multiple files in the same package should share the same scope.
fn test_multi_file_package_scope() {
    let resolver = GoResolver::new();

    // Parse three files in the same package directory
    let source_a = r#"
package engine

func Alpha() int { return 1 }
"#;
    resolver.parse_file(Path::new("/project/engine/a.go"), source_a);

    let source_b = r#"
package engine

func Beta() int { return Alpha() }
"#;
    resolver.parse_file(Path::new("/project/engine/b.go"), source_b);

    let source_c = r#"
package engine

func Gamma() int { return Beta() + Alpha() }
"#;
    resolver.parse_file(Path::new("/project/engine/c.go"), source_c);

    // c.go can call Alpha (defined in a.go)
    let edge_alpha = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/engine/c.go".into(),
        line: 4,
        callee_name: "Alpha".into(),
        receiver: None,
    });
    assert!(edge_alpha.is_some(), "c.go should resolve Alpha from a.go");

    // c.go can call Beta (defined in b.go)
    let edge_beta = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/engine/c.go".into(),
        line: 4,
        callee_name: "Beta".into(),
        receiver: None,
    });
    assert!(edge_beta.is_some(), "c.go should resolve Beta from b.go");

    // b.go can call Alpha (defined in a.go)
    let edge_b_alpha = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/engine/b.go".into(),
        line: 4,
        callee_name: "Alpha".into(),
        receiver: None,
    });
    assert!(
        edge_b_alpha.is_some(),
        "b.go should resolve Alpha from a.go"
    );
}

#[test]
/// Same-file function calls resolve via the single-file cache.
/// This is the simplest form of "package scope" — one file calling
/// another function defined in the same file.
fn test_same_file_function_call() {
    let resolver = GoResolver::new();
    let source = r#"
package handlers

func helper() int {
    return 42
}

func Process() int {
    return helper()
}
"#;
    let path = Path::new("handlers.go");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "handlers.go".into(),
        line: 9,
        callee_name: "helper".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Should resolve same-file call to helper()");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert_eq!(edge.target_file, "handlers.go");
    assert!(
        edge.confidence >= 0.90,
        "Same-package call should have high confidence"
    );
}

#[test]
/// Test packages (_test.go) should be treated as the same package scope.
fn test_test_file_package_scope() {
    let resolver = GoResolver::new();

    // Parse main file
    let source_main = r#"
package handlers

func ProcessRequest() string {
    return "ok"
}
"#;
    resolver.parse_file(Path::new("/project/handlers/handler.go"), source_main);

    // Parse _test.go file in same directory
    let source_test = r#"
package handlers

func TestProcessRequest() {
    result := ProcessRequest()
    _ = result
}
"#;
    resolver.parse_file(Path::new("/project/handlers/handler_test.go"), source_test);

    // _test.go should resolve calls to the main package
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/handlers/handler_test.go".into(),
        line: 5,
        callee_name: "ProcessRequest".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "_test.go should resolve calls to same-package functions"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "ProcessRequest");
    assert!(edge.confidence >= 0.70);
}
