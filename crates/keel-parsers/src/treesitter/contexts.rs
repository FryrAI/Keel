//! Ancestry-derived definition flags shared by tree-sitter languages.

use super::{is_typescript_family, node_text};

/// The four ancestry-derived flags a definition carries.
#[derive(Default)]
pub(super) struct DefContexts {
    pub(super) in_test: bool,
    pub(super) in_trait: bool,
    pub(super) is_associated: bool,
    pub(super) is_decorated: bool,
}

/// Node kinds that introduce a new function scope, per supported grammar.
fn is_function_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "function_definition"
            | "method_declaration"
    )
}

/// Compute every ancestry-derived flag in one walk up the tree.
pub(super) fn definition_contexts(
    node: tree_sitter::Node<'_>,
    lang: &str,
    source: &[u8],
) -> DefContexts {
    let mut ctx = DefContexts::default();
    let mut trait_decided = false;
    let mut assoc_decided = false;
    let mut decorator_decided = false;
    let is_ts = is_typescript_family(lang);

    let mut current = Some(node);
    let mut is_self = true;
    while let Some(n) = current {
        let kind = n.kind();

        if !ctx.in_test {
            match lang {
                "rust"
                    if matches!(kind, "function_item" | "mod_item")
                        && preceding_attrs_mark_test(n, source) =>
                {
                    ctx.in_test = true;
                }
                "python" if python_marks_test(n, source) => ctx.in_test = true,
                _ if is_ts && kind == "call_expression" && ts_call_is_test_block(n, source) => {
                    ctx.in_test = true;
                }
                _ => {}
            }
        }

        if !is_self {
            if !trait_decided {
                match (lang, kind) {
                    ("rust", "trait_item") => {
                        ctx.in_trait = true;
                        trait_decided = true;
                    }
                    ("rust", "impl_item") => {
                        ctx.in_trait = n.child_by_field_name("trait").is_some();
                        trait_decided = true;
                    }
                    _ if is_ts && kind == "interface_declaration" => {
                        ctx.in_trait = true;
                        trait_decided = true;
                    }
                    _ if is_ts && matches!(kind, "class_declaration" | "class") => {
                        ctx.in_trait = has_implements_clause(n);
                        trait_decided = true;
                    }
                    _ => {}
                }
            }

            if !assoc_decided {
                if is_function_scope(kind) {
                    assoc_decided = true;
                } else if matches!(
                    (lang, kind),
                    ("rust", "impl_item") | ("rust", "trait_item") | ("python", "class_definition")
                ) || matches!(kind, "class_body" | "class_declaration" | "class")
                {
                    ctx.is_associated = true;
                    assoc_decided = true;
                }
            }

            if !decorator_decided {
                if is_function_scope(kind) {
                    decorator_decided = true;
                } else if lang == "python" && kind == "decorated_definition" {
                    ctx.is_decorated = true;
                    decorator_decided = true;
                }
            }
        }

        is_self = false;
        current = n.parent();
    }
    ctx
}

/// True when a TypeScript class carries an `implements` clause.
fn has_implements_clause(class: tree_sitter::Node<'_>) -> bool {
    (0..class.child_count())
        .filter_map(|i| class.child(i))
        .filter(|c| c.kind() == "class_heritage")
        .any(|heritage| {
            (0..heritage.child_count()).any(|i| {
                heritage
                    .child(i)
                    .is_some_and(|c| c.kind() == "implements_clause")
            })
        })
}

/// True when a Python node marks a pytest/unittest context on its own.
fn python_marks_test(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match n.kind() {
        "function_definition" => n
            .child_by_field_name("name")
            .is_some_and(|name| node_text(name, source).starts_with("test_")),
        "class_definition" => n
            .child_by_field_name("superclasses")
            .is_some_and(|bases| node_text(bases, source).contains("TestCase")),
        _ => false,
    }
}

/// True when a TS call is a `describe`/`it`/`test` block.
fn ts_call_is_test_block(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(func) = n.child_by_field_name("function") else {
        return false;
    };
    let callee = match func.kind() {
        "identifier" => node_text(func, source),
        "member_expression" => func
            .child_by_field_name("object")
            .map(|o| node_text(o, source))
            .unwrap_or(""),
        _ => return false,
    };
    matches!(callee, "describe" | "it" | "test")
}

/// Scan Rust attributes immediately preceding an item for a test marker.
fn preceding_attrs_mark_test(item: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut sibling = item.prev_sibling();
    while let Some(node) = sibling {
        match node.kind() {
            "attribute_item" => {
                let compact: String = node_text(node, source)
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if compact.contains("cfg(test")
                    || compact == "#[test]"
                    || compact.starts_with("#[test(")
                    || compact.ends_with("::test]")
                    || compact.contains("::test(")
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = node.prev_sibling();
    }
    false
}

/// True when `keel:keep` appears on this definition's line or the line above.
pub(super) fn has_keep_marker(lines: &[&str], line_start: u32) -> bool {
    let own = line_start
        .checked_sub(1)
        .and_then(|i| lines.get(i as usize));
    let above = line_start
        .checked_sub(2)
        .and_then(|i| lines.get(i as usize));
    own.is_some_and(|line| line.contains("keel:keep"))
        || above.is_some_and(|line| line.contains("keel:keep"))
}
