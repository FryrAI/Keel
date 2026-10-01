//! Callable identity and pure template body extraction, shared by map/review.
use super::ast::HomeSpan;
use super::literals::{children, template_segments, text};
use super::{eligible_segment, MIN_SEGMENT_CHARS};
use keel_core::template_homes::TemplateHome;
use keel_parsers::resolver::Definition;
use std::collections::HashMap;
use tree_sitter::Node;

fn callable(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "method_declaration"
    )
}

fn only_expression<'a>(node: Node<'a>, lang: &str) -> Option<Node<'a>> {
    let body = node.child_by_field_name("body")?;
    if !matches!(body.kind(), "block" | "statement_block") {
        return (node.kind() == "arrow_function").then_some(body);
    }
    let mut statements = children(body);
    if lang == "go" && statements.len() == 1 && statements[0].kind() == "statement_list" {
        statements = children(statements[0]);
    }
    let [mut expr] = statements.as_slice() else {
        return None;
    };
    if expr.kind() == "expression_statement" {
        // A Rust expression ending in ';' returns (), unless it is `return`.
        if lang == "rust" && expr.named_child(0)?.kind() != "return_expression" {
            return None;
        }
        expr = expr.named_child(0)?;
    }
    if matches!(expr.kind(), "return_statement" | "return_expression") {
        let parts = children(expr);
        let [value] = parts.as_slice() else {
            return None;
        };
        expr = *value;
        if expr.kind() == "expression_list" {
            let parts = children(expr);
            let [value] = parts.as_slice() else {
                return None;
            };
            expr = *value;
        }
    } else if lang != "rust" {
        return None;
    }
    while expr.kind() == "parenthesized_expression" {
        let parts = children(expr);
        let [value] = parts.as_slice() else {
            return None;
        };
        expr = *value;
    }
    Some(expr)
}

fn callable_name(node: Node<'_>, source: &str) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(text(name, source).into());
    }
    let mut parent = node.parent();
    while let Some(n) = parent {
        match n.kind() {
            "variable_declarator" | "pair" => {
                return n
                    .child_by_field_name("name")
                    .or_else(|| n.child_by_field_name("key"))
                    .map(|name| text(name, source).into());
            }
            "export_statement"
                if (0..n.child_count())
                    .filter_map(|i| n.child(i))
                    .any(|child| child.kind() == "default") =>
            {
                return Some("default".into());
            }
            "parenthesized_expression" => parent = n.parent(),
            _ => break,
        }
    }
    None
}

/// Index definitions once by their callable end line.
pub(super) fn definition_index(definitions: &[Definition]) -> HashMap<u32, Vec<&Definition>> {
    let mut index = HashMap::<_, Vec<_>>::new();
    for def in definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
    {
        index.entry(def.line_end).or_default().push(def);
    }
    index
}

/// Capture any current callable body, including ineligible cached owners.
pub(super) fn span(
    node: Node<'_>,
    file: &str,
    source: &str,
    lang: &str,
    fmt: bool,
    index: &HashMap<u32, Vec<&Definition>>,
) -> Option<HomeSpan> {
    if !callable(node.kind()) {
        return None;
    }
    let body = node.child_by_field_name("body")?;
    let def = index
        .get(&(node.end_position().row as u32 + 1))
        .and_then(|defs| defs.iter().find(|d| d.body_text == text(body, source)));
    let name = def
        .map(|d| d.name.clone())
        .or_else(|| callable_name(node, source))?;
    let pieces = if node.has_error() {
        None
    } else {
        only_expression(node, lang).and_then(|expr| template_segments(expr, source, lang, fmt))
    };
    let eligible = pieces.is_some();
    let mut segments: Vec<_> = pieces
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| eligible_segment(s, MIN_SEGMENT_CHARS))
        .collect();
    segments.sort();
    segments.dedup();
    Some(HomeSpan {
        home: TemplateHome {
            // Ineligible spans are used only for body/name exclusion, never
            // stored or reported; hashing their often-large bodies buys nothing.
            hash: if eligible {
                def.map(|d| d.hash())
                    .unwrap_or_else(|| keel_core::hash::compute_hash(&name, text(node, source), ""))
            } else {
                String::new()
            },
            name,
            file: file.into(),
            line: node.start_position().row as u32 + 1,
            segments,
        },
        start: body.start_byte(),
        end: body.end_byte(),
        eligible,
    })
}
