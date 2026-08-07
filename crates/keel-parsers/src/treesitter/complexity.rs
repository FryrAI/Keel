//! McCabe cyclomatic complexity per definition (issue #58).
//!
//! `1 + decision points`, counted straight off the tree-sitter subtree — no
//! control-flow graph, no dataflow. The count stops at a nested *named*
//! definition, because that definition gets its own [`Definition`] and its own
//! reading; folding its branches into the enclosing function would make an
//! outer function look complex for code it merely encloses.
//!
//! [`Definition`]: crate::resolver::Definition

use super::is_typescript_family;

/// Cyclomatic complexity of `body`: `1 + decision points` in its subtree.
///
/// `lang` is a [`super::detect_language`] result. An unrecognized language has
/// no decision table and scores 1 — no reading is better than a wrong one.
pub(crate) fn compute(body: tree_sitter::Node<'_>, lang: &str) -> u32 {
    1 + count_decisions(body, lang)
}

/// Decision points in `node`'s children, recursively, stopping at nested
/// definition boundaries.
fn count_decisions(node: tree_sitter::Node<'_>, lang: &str) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        if is_boundary(kind, lang) {
            continue;
        }
        if is_decision(kind, lang) || is_short_circuit(child, lang) {
            count += 1;
        }
        count += count_decisions(child, lang);
    }
    count
}

/// Whether a node kind opens a nested definition that carries its own count.
///
/// Go has none: the language disallows nested `func` declarations. TS arrow
/// functions are deliberately absent — the inline callback form
/// (`.map(x => ...)`) is not captured as a separate definition and correctly
/// folds into its enclosing function; the narrow
/// `const f = (x) => ...` form is captured and so double-counts, an accepted
/// approximation rather than a special case that would break the common one.
fn is_boundary(kind: &str, lang: &str) -> bool {
    match lang {
        "rust" => kind == "function_item",
        "python" => kind == "function_definition",
        l if is_typescript_family(l) => {
            matches!(kind, "function_declaration" | "method_definition")
        }
        _ => false,
    }
}

/// Whether a node kind is a branch, loop, or arm — one decision point.
///
/// Rust `if let`/`while let` need no entry: the grammar spells them as
/// ordinary `if_expression`/`while_expression` with a `let_condition` in the
/// `condition` field, so they are already counted once.
fn is_decision(kind: &str, lang: &str) -> bool {
    match lang {
        "rust" => matches!(
            kind,
            "if_expression"
                | "while_expression"
                | "for_expression"
                | "loop_expression"
                | "match_arm"
                | "try_expression"
        ),
        "python" => matches!(
            kind,
            "if_statement"
                | "elif_clause"
                | "for_statement"
                | "while_statement"
                | "except_clause"
                | "except_group_clause"
                | "conditional_expression"
                | "case_clause"
        ),
        "go" => matches!(
            kind,
            "if_statement"
                | "for_statement"
                | "expression_case"
                | "type_case"
                | "communication_case"
        ),
        l if is_typescript_family(l) => matches!(
            kind,
            "if_statement"
                | "for_statement"
                | "for_in_statement"
                | "while_statement"
                | "do_statement"
                | "catch_clause"
                | "switch_case"
                | "ternary_expression"
        ),
        _ => false,
    }
}

/// Whether this node is a short-circuiting boolean operator — an extra path
/// through the function, so an extra decision point.
///
/// Python spells it as its own node kind (`boolean_operator`, only ever `and`
/// or `or`); the C-family grammars fold it into `binary_expression`, so the
/// operator field has to be read to tell `a && b` from `a + b`.
fn is_short_circuit(node: tree_sitter::Node<'_>, lang: &str) -> bool {
    if lang == "python" {
        return node.kind() == "boolean_operator";
    }
    node.kind() == "binary_expression"
        && node
            .child_by_field_name("operator")
            .is_some_and(|op| matches!(op.kind(), "&&" | "||"))
}
