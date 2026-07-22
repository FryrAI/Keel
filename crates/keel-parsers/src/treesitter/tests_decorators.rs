//! `Definition::is_decorated` (Python decorator registration) and
//! `Definition::has_keep_marker` (`keel:keep` per-symbol W005 escape hatch).
//!
//! Both flags exist for the same reason: a function can be genuinely "used"
//! in a way the call graph cannot see — handed to a decorator/framework, or
//! dispatched through `globals()`/`getattr` — and W005's "no callers" signal
//! is an artifact of the analysis rather than real dead code.

use std::path::Path;

use super::*;

fn find<'a>(defs: &'a [Definition], name: &str) -> &'a Definition {
    defs.iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("missing def {name}"))
}

#[test]
fn python_decorated_function_is_marked() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
@register("evt")
def handler():
    pass

def plain():
    pass
"#;
    let defs = parser
        .parse_file("python", Path::new("handlers.py"), source)
        .unwrap()
        .definitions;

    assert!(
        find(&defs, "handler").is_decorated,
        "a @register(...)-decorated function is is_decorated"
    );
    assert!(
        !find(&defs, "plain").is_decorated,
        "an undecorated function is not is_decorated"
    );
}

#[test]
fn python_nested_helper_inside_decorated_function_is_not_decorated() {
    let mut parser = TreeSitterParser::new();
    // The decorator wraps `handler`, not the helper nested in its body — a
    // function scope sits between the helper and the `decorated_definition`
    // node, so the decorator's reach must not propagate to it.
    let source = r#"
@register("evt")
def handler():
    def _inner():
        return 1
    return _inner()
"#;
    let defs = parser
        .parse_file("python", Path::new("handlers.py"), source)
        .unwrap()
        .definitions;

    assert!(find(&defs, "handler").is_decorated);
    assert!(
        !find(&defs, "_inner").is_decorated,
        "a helper nested inside a decorated function's body is not itself decorated"
    );
}

#[test]
fn keep_marker_above_definition_is_detected() {
    let mut parser = TreeSitterParser::new();
    let source = "# keel:keep\ndef dynamic_handler():\n    pass\n";
    let defs = parser
        .parse_file("python", Path::new("dispatch.py"), source)
        .unwrap()
        .definitions;
    assert!(find(&defs, "dynamic_handler").has_keep_marker);
}

#[test]
fn keep_marker_trailing_on_definition_line_is_detected() {
    let mut parser = TreeSitterParser::new();
    let source = "def dynamic_handler():  # keel:keep\n    pass\n";
    let defs = parser
        .parse_file("python", Path::new("dispatch.py"), source)
        .unwrap()
        .definitions;
    assert!(find(&defs, "dynamic_handler").has_keep_marker);
}

#[test]
fn unrelated_comment_above_definition_does_not_mark() {
    let mut parser = TreeSitterParser::new();
    let source = "# just a helper\ndef normal_fn():\n    pass\n";
    let defs = parser
        .parse_file("python", Path::new("plain.py"), source)
        .unwrap()
        .definitions;
    assert!(!find(&defs, "normal_fn").has_keep_marker);
}

#[test]
fn keep_marker_works_across_languages() {
    // The scan is a dumb substring check with no per-language comment syntax
    // to track — prove it also fires for Rust's `//` comment form.
    let mut parser = TreeSitterParser::new();
    let source = "// keel:keep\nfn dead_looking() -> i32 {\n    1\n}\n";
    let defs = parser
        .parse_file("rust", Path::new("lib.rs"), source)
        .unwrap()
        .definitions;
    assert!(find(&defs, "dead_looking").has_keep_marker);
}
