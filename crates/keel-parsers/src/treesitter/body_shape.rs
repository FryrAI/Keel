//! Trivial-wrapper body-shape detection (issue #60).
//!
//! A body is a "trivial wrapper" when — modulo a leading Python docstring or
//! a bare `pass` — it reduces to exactly one statement, and that statement is
//! nothing but a delegating call: a bare call, `return call(...)`, or (TS/JS)
//! a concise arrow body that IS the call. Detected once at parse time,
//! against the real AST node, because the flattened `body_text` string
//! [`Definition`](crate::resolver::Definition) otherwise carries cannot tell
//! "one call" from "one call plus a comment" without re-parsing.
//!
//! Node-kind strings below were confirmed against real `to_sexp()` dumps for
//! each language (see issue #60's implementation notes) rather than assumed
//! from grammar docs — in particular, Python's bare-call node is named
//! `call` (not `call_expression`), and Go wraps its `return` value in an
//! `expression_list` even for a single value.

/// True when `body_node`'s shape is exactly one delegating call statement.
///
/// `None` (no body — e.g. module-level definitions) is never trivial.
/// `source` is accepted (unused today) to keep this signature aligned with
/// the other per-definition detectors in this module, which all take the
/// full source buffer.
pub(crate) fn is_trivial_wrapper_body(
    body_node: Option<tree_sitter::Node<'_>>,
    lang: &str,
    _source: &[u8],
) -> bool {
    let Some(body) = body_node else {
        return false;
    };

    // TS/JS concise arrow body IS the expression itself: `(x) => helper(x)`.
    if body.kind() == "call_expression" {
        return true;
    }

    if !matches!(body.kind(), "block" | "statement_block") {
        return false;
    }

    let mut cursor = body.walk();
    let mut statements: Vec<tree_sitter::Node<'_>> = body.named_children(&mut cursor).collect();
    statements.retain(|n| !n.kind().contains("comment"));

    if lang == "python" {
        if is_python_docstring_statement(statements.first().copied()) {
            statements.remove(0);
        }
        statements.retain(|n| n.kind() != "pass_statement");
    }

    let [only] = statements.as_slice() else {
        return false;
    };
    is_call_like(*only)
}

/// Whether `node` is the Python docstring statement — the shape
/// `docstrings::extract_python_docstring` already keys off:
/// `expression_statement > string`.
fn is_python_docstring_statement(node: Option<tree_sitter::Node<'_>>) -> bool {
    node.is_some_and(|n| {
        n.kind() == "expression_statement" && n.named_child(0).map(|c| c.kind()) == Some("string")
    })
}

/// A node kind that is itself a call expression — Rust/Go/TS
/// `call_expression`, or Python's differently named `call`.
fn is_call_shape(kind: &str) -> bool {
    matches!(kind, "call_expression" | "call")
}

/// True when `node` is a statement whose entire content is a delegating
/// call — a bare call statement, or a `return` of one.
fn is_call_like(node: tree_sitter::Node<'_>) -> bool {
    if is_call_shape(node.kind()) {
        return true;
    }
    match node.kind() {
        // Go/TS/Python `return call(...)`. Go additionally wraps the
        // returned value in an `expression_list`, even for a single value.
        "return_statement" => match node.named_child(0) {
            Some(n) if is_call_shape(n.kind()) => true,
            Some(n) if n.kind() == "expression_list" => {
                n.named_child(0).is_some_and(|c| is_call_shape(c.kind()))
            }
            _ => false,
        },
        // Python bare call statement (`helper(x)`); or Rust `return
        // helper(x);` — Rust spells a semicolon-terminated return as an
        // `expression_statement` wrapping a `return_expression`.
        "expression_statement" => match node.named_child(0) {
            Some(n) if is_call_shape(n.kind()) => true,
            Some(n) if n.kind() == "return_expression" => {
                n.named_child(0).is_some_and(|c| is_call_shape(c.kind()))
            }
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::treesitter::TreeSitterParser;

    /// Parses `src` as `lang` and returns whether its sole top-level
    /// definition's body is a trivial wrapper.
    fn check(lang: &str, src: &str) -> bool {
        let mut parser = TreeSitterParser::new();
        let tree = parser.parse(lang, src.as_bytes()).expect("parse failed");
        let root = tree.root_node();
        let body = find_body(root, lang).expect("no body node found in fixture");
        is_trivial_wrapper_body(Some(body), lang, src.as_bytes())
    }

    /// Locates the `block`/`statement_block`/arrow-expression body of the
    /// first function-shaped node in the tree, depth-first.
    fn find_body<'a>(node: tree_sitter::Node<'a>, lang: &str) -> Option<tree_sitter::Node<'a>> {
        let block_kinds: &[&str] = &["block", "statement_block"];
        if block_kinds.contains(&node.kind()) && node.parent().is_some() {
            return Some(node);
        }
        if (lang.starts_with("type") || lang == "javascript") && node.kind() == "arrow_function" {
            if let Some(body) = node.child_by_field_name("body") {
                return Some(body);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_body(child, lang) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn python_bare_call_is_trivial() {
        assert!(check("python", "def f(x):\n    helper(x)\n"));
    }

    #[test]
    fn python_return_call_is_trivial() {
        assert!(check("python", "def f(x):\n    return helper(x)\n"));
    }

    #[test]
    fn python_docstring_then_return_call_is_trivial() {
        assert!(check(
            "python",
            "def f(x):\n    \"\"\"doc\"\"\"\n    return helper(x)\n"
        ));
    }

    #[test]
    fn python_pass_alone_is_not_trivial() {
        assert!(!check("python", "def f(x):\n    pass\n"));
    }

    #[test]
    fn python_two_statements_is_not_trivial() {
        assert!(!check(
            "python",
            "def f(x):\n    y = helper(x)\n    return y\n"
        ));
    }

    #[test]
    fn python_binary_expression_is_not_trivial() {
        assert!(!check("python", "def f(x):\n    return x + 1\n"));
    }

    #[test]
    fn rust_bare_tail_call_is_trivial() {
        assert!(check("rust", "fn f(x: i32) -> i32 {\n    helper(x)\n}\n"));
    }

    #[test]
    fn rust_return_call_is_trivial() {
        assert!(check(
            "rust",
            "fn f(x: i32) -> i32 {\n    return helper(x);\n}\n"
        ));
    }

    #[test]
    fn rust_binary_expression_is_not_trivial() {
        assert!(!check("rust", "fn f(x: i32) -> i32 {\n    x + 1\n}\n"));
    }

    #[test]
    fn go_return_call_is_trivial() {
        assert!(check(
            "go",
            "func f(x int) int {\n    return helper(x)\n}\n"
        ));
    }

    #[test]
    fn go_bare_call_is_trivial() {
        assert!(check("go", "func f(x int) {\n    helper(x)\n}\n"));
    }

    #[test]
    fn typescript_return_call_is_trivial() {
        assert!(check(
            "typescript",
            "function f(x: number): number {\n    return helper(x);\n}\n"
        ));
    }

    #[test]
    fn typescript_concise_arrow_is_trivial() {
        assert!(check("typescript", "const f = (x: number) => helper(x);\n"));
    }

    #[test]
    fn typescript_block_arrow_return_call_is_trivial() {
        assert!(check(
            "typescript",
            "const f = (x: number) => { return helper(x); };\n"
        ));
    }

    #[test]
    fn typescript_binary_expression_arrow_is_not_trivial() {
        assert!(!check("typescript", "const f = (x: number) => x + 1;\n"));
    }

    #[test]
    fn no_body_is_not_trivial() {
        assert!(!is_trivial_wrapper_body(None, "rust", b""));
    }
}
