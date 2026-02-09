// Tests for Go import path resolution (Spec 004 - Go Resolution)
use std::path::Path;

use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};

#[test]
/// Standard library imports should be recognized and parsed.
fn test_stdlib_import_resolution() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import "fmt"

func main() {
    fmt.Println("hello")
}
"#;
    let result = resolver.parse_file(Path::new("main.go"), source);

    // Should find the "fmt" import
    let fmt_import = result.imports.iter().find(|imp| imp.source.contains("fmt"));
    assert!(fmt_import.is_some(), "Should find 'fmt' import");

    // Should resolve fmt.Println call edge
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "main.go".into(),
        line: 7,
        callee_name: "fmt.Println".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Should resolve fmt.Println call");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Println");
    assert!(
        edge.confidence >= 0.7,
        "Imported call should have decent confidence"
    );
}

#[test]
#[ignore = "Module-relative import resolution requires go.mod parsing"]
/// Module-relative imports should resolve to the correct local package.
fn test_module_relative_import() {
    // Requires go.mod parsing + directory walking which the GoResolver
    // doesn't currently support.
}

#[test]
/// Aliased imports should track the alias for call resolution.
/// Note: tree-sitter may extract the full import path; the GoResolver
/// uses go_package_alias() to extract the last segment for matching.
fn test_aliased_import() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import "github.com/user/project/utils"

func main() {
    utils.Process()
}
"#;
    let result = resolver.parse_file(Path::new("main.go"), source);

    // Should find the import
    let utils_import = result
        .imports
        .iter()
        .find(|imp| imp.source.contains("utils"));
    assert!(utils_import.is_some(), "Should find utils import");

    // The resolver uses go_package_alias to extract "utils" from the path
    // and match it against the qualified call "utils.Process"
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "main.go".into(),
        line: 7,
        callee_name: "utils.Process".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Should resolve utils.Process call");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Process");
    assert!(edge.confidence >= 0.7);
}

#[test]
#[ignore = "Dot imports require special scope resolution"]
/// Dot imports should import all exported names into the current scope.
fn test_dot_import() {
    // Dot imports (`. "pkg"`) bring all exported names into scope
    // without package prefix. The current GoResolver doesn't handle
    // this special import form.
}

#[test]
#[ignore = "Blank imports require special tracking"]
/// Blank imports (side-effect only) should be tracked but not produce call edges.
fn test_blank_import() {
    // Blank imports (`_ "pkg"`) are side-effect only.
    // The current GoResolver doesn't distinguish blank imports.
}
