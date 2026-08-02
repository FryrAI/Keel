//! One import statement, every binding: regression tests for the dedup that
//! used to keep only the first specifier of a multi-name import.
//!
//! `import { a, b, c } from './m'` produces one query match per specifier, all
//! reporting the same `(source, line)`. Collapsing them to the first match lost
//! `b` and `c` from the file's import record, and `resolve_cross_file_call`
//! resolves a reference only when some import names it — so every use of `b`
//! and `c` went unresolved and their definitions read as dead code.

use std::path::Path;

use super::*;

/// Parse `source` as `lang` and return the names of the import whose source
/// specifier contains `needle`.
fn import_names(lang: &str, file: &str, source: &str, needle: &str) -> Vec<String> {
    let mut parser = TreeSitterParser::new();
    let result = parser.parse_file(lang, Path::new(file), source).unwrap();
    result
        .imports
        .iter()
        .find(|i| i.source.contains(needle))
        .unwrap_or_else(|| panic!("no import matching {needle} in {:?}", result.imports))
        .imported_names
        .clone()
}

#[test]
fn typescript_named_import_keeps_every_specifier() {
    let names = import_names(
        "typescript",
        "/x.ts",
        "import { alpha, beta, gamma } from './m';\n",
        "./m",
    );
    for want in ["alpha", "beta", "gamma"] {
        assert!(
            names.contains(&want.to_string()),
            "missing {want}: {names:?}"
        );
    }
}

#[test]
fn typescript_multiline_named_import_keeps_every_specifier() {
    // The production shape: one specifier per line, trailing `type` imports.
    let names = import_names(
        "typescript",
        "/x.ts",
        "import {\n  applyView,\n  completenessPct,\n  matchesQuery,\n  type OverviewRow\n} from '$lib/portfolio/model';\n",
        "model",
    );
    for want in ["applyView", "completenessPct", "matchesQuery"] {
        assert!(
            names.contains(&want.to_string()),
            "missing {want}: {names:?}"
        );
    }
}

#[test]
fn typescript_default_plus_named_import_keeps_both() {
    let names = import_names(
        "typescript",
        "/x.ts",
        "import Widget, { helper } from './w';\n",
        "./w",
    );
    for want in ["Widget", "helper"] {
        assert!(
            names.contains(&want.to_string()),
            "missing {want}: {names:?}"
        );
    }
}

#[test]
fn typescript_namespace_import_records_its_alias() {
    let names = import_names(
        "typescript",
        "/x.ts",
        "import * as api from './a';\n",
        "./a",
    );
    assert!(
        names.contains(&"api".to_string()),
        "namespace alias is the file's local binding: {names:?}"
    );
}

#[test]
fn python_from_import_keeps_every_name() {
    let names = import_names("python", "/x.py", "from mod import a, b, c\n", "mod");
    for want in ["a", "b", "c"] {
        assert!(
            names.contains(&want.to_string()),
            "missing {want}: {names:?}"
        );
    }
}

#[test]
fn go_blank_and_dot_markers_survive_the_merge() {
    // The marker patterns and the generic pattern match the same single
    // binding, so the marker must replace the package name, not join it.
    let mut parser = TreeSitterParser::new();
    let source = "package m\nimport (\n\t\"fmt\"\n\t_ \"embed\"\n\t. \"errors\"\n)\n";
    let result = parser.parse_file("go", Path::new("/x.go"), source).unwrap();
    let blank = result
        .imports
        .iter()
        .find(|i| i.source.contains("embed"))
        .expect("blank import present");
    assert_eq!(blank.imported_names, vec!["_".to_string()]);
    let dot = result
        .imports
        .iter()
        .find(|i| i.source.contains("errors"))
        .expect("dot import present");
    assert_eq!(dot.imported_names, vec![".".to_string()]);
}
