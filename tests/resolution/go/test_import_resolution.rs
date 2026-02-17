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
/// Module-relative imports should extract the correct package alias.
fn test_module_relative_import() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import "mymod/internal/pkg"

func main() {
    pkg.DoStuff()
}
"#;
    let result = resolver.parse_file(Path::new("main.go"), source);

    // Should find the module-relative import
    let pkg_import = result
        .imports
        .iter()
        .find(|imp| imp.source.contains("internal/pkg"));
    assert!(
        pkg_import.is_some(),
        "Should find module-relative import, got: {:?}",
        result.imports
    );

    // Package alias should be "pkg" (last path segment)
    let imp = pkg_import.unwrap();
    assert!(
        imp.imported_names.contains(&"pkg".to_string()),
        "imported_names should contain 'pkg', got: {:?}",
        imp.imported_names
    );

    // Should resolve pkg.DoStuff call via alias
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "main.go".into(),
        line: 7,
        callee_name: "pkg.DoStuff".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Should resolve pkg.DoStuff call");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "DoStuff");
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
/// Dot imports should import all exported names into the current scope.
fn test_dot_import() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import . "math"

func main() {
    Sqrt(16.0)
}
"#;
    let result = resolver.parse_file(Path::new("dot.go"), source);

    // Should find the dot import with "." marker
    let dot_import = result
        .imports
        .iter()
        .find(|imp| imp.imported_names.contains(&".".to_string()));
    assert!(
        dot_import.is_some(),
        "Should find dot import with '.' marker, got: {:?}",
        result.imports
    );
    assert_eq!(dot_import.unwrap().source, "math");

    // Unqualified call Sqrt should resolve through dot import
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "dot.go".into(),
        line: 7,
        callee_name: "Sqrt".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "Sqrt should resolve via dot import");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Sqrt");
    assert_eq!(edge.target_file, "math");
    assert!(
        edge.confidence >= 0.5,
        "dot-import resolution should have reasonable confidence"
    );
}

#[test]
/// Blank imports (side-effect only) should be tracked but not produce call edges.
fn test_blank_import() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import _ "database/sql"

func main() {
    sql.Open("driver", "dsn")
}
"#;
    let result = resolver.parse_file(Path::new("blank.go"), source);

    // Should find the blank import with "_" marker
    let blank_import = result
        .imports
        .iter()
        .find(|imp| imp.imported_names.contains(&"_".to_string()));
    assert!(
        blank_import.is_some(),
        "Should find blank import with '_' marker, got: {:?}",
        result.imports
    );
    assert_eq!(blank_import.unwrap().source, "database/sql");

    // Qualified call sql.Open should NOT resolve (blank import is side-effect only)
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "blank.go".into(),
        line: 7,
        callee_name: "sql.Open".into(),
        receiver: None,
    });
    assert!(
        edge.is_none(),
        "Blank import should not produce call edges"
    );
}
