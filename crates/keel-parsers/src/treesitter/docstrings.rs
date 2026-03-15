//! Docstring extraction from tree-sitter AST nodes.

use super::node_text;

/// Extract a doc comment from the preceding siblings of a definition node.
///
/// Handles:
/// - Rust: `///` line comments and `/** */` block comments
/// - TypeScript/JS: `/** */` JSDoc comments
/// - Go: `//` comment blocks immediately preceding the definition
/// - Python: triple-quoted string as the first statement in the function body
pub fn extract_docstring(
    def_node: tree_sitter::Node<'_>,
    body_node: Option<tree_sitter::Node<'_>>,
    source: &[u8],
) -> Option<String> {
    // Strategy 1: Check preceding siblings for comment nodes (Rust, TS, Go)
    if let Some(doc) = extract_preceding_doc_comment(def_node, source) {
        return Some(doc);
    }

    // Strategy 1b: For exported/decorated definitions, the doc comment may be a
    // sibling of the parent wrapper node (e.g. export_statement, decorated_definition)
    if let Some(parent) = def_node.parent() {
        let pk = parent.kind();
        if pk == "export_statement" || pk == "decorated_definition" || pk == "declaration_list" {
            if let Some(doc) = extract_preceding_doc_comment(parent, source) {
                return Some(doc);
            }
        }
    }

    // Strategy 2: Python docstrings — first expression_statement > string in body
    if let Some(body) = body_node {
        if let Some(doc) = extract_python_docstring(body, source) {
            return Some(doc);
        }
    }

    None
}

/// Walk backwards from a node through preceding siblings (skipping attributes)
/// to collect doc comment lines.
pub fn extract_preceding_doc_comment(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut sibling = node.prev_sibling();

    // Walk backwards, collecting comment nodes, skipping attribute nodes
    while let Some(sib) = sibling {
        let kind = sib.kind();
        match kind {
            // Rust: /// or //! doc comments
            "line_comment" => {
                let text = node_text(sib, source).trim_end();
                if text.starts_with("///") {
                    let content = text
                        .strip_prefix("/// ")
                        .unwrap_or(text.strip_prefix("///").unwrap_or(text));
                    doc_lines.push(content.to_string());
                } else {
                    // Regular // comment — stop
                    break;
                }
            }
            // Rust: /** */ doc block comments
            "block_comment" => {
                let text = node_text(sib, source);
                if text.starts_with("/**") {
                    let cleaned = text
                        .strip_prefix("/**")
                        .and_then(|s| s.strip_suffix("*/"))
                        .unwrap_or(text)
                        .trim();
                    if !cleaned.is_empty() {
                        doc_lines.push(cleaned.to_string());
                    }
                }
                break;
            }
            // TypeScript/JS/Go: comment node
            "comment" => {
                let text = node_text(sib, source).trim_end();
                if text.starts_with("/**") {
                    // JSDoc block comment
                    let cleaned = text
                        .strip_prefix("/**")
                        .and_then(|s| s.strip_suffix("*/"))
                        .unwrap_or(text)
                        .trim();
                    if !cleaned.is_empty() {
                        doc_lines.push(cleaned.to_string());
                    }
                    break;
                } else if text.starts_with("//") {
                    // Go-style // comment blocks
                    let content = text
                        .strip_prefix("// ")
                        .unwrap_or(text.strip_prefix("//").unwrap_or(text));
                    doc_lines.push(content.to_string());
                } else {
                    break;
                }
            }
            // Skip attributes (#[...]) and decorators (@...) between doc and def
            "attribute_item" | "attribute" | "decorator" | "inner_attribute_item" => {}
            _ => break,
        }
        sibling = sib.prev_sibling();
    }

    if doc_lines.is_empty() {
        return None;
    }
    // Reverse because we walked backwards
    doc_lines.reverse();
    Some(doc_lines.join("\n"))
}

/// Extract a Python docstring from the first statement of a function body.
pub fn extract_python_docstring(body_node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let first_child = body_node.named_child(0)?;
    if first_child.kind() != "expression_statement" {
        return None;
    }
    let string_node = first_child.named_child(0)?;
    if string_node.kind() != "string" {
        return None;
    }
    let text = node_text(string_node, source);
    // Strip triple-quote delimiters
    let content = text
        .strip_prefix("\"\"\"")
        .and_then(|s| s.strip_suffix("\"\"\""))
        .or_else(|| text.strip_prefix("'''").and_then(|s| s.strip_suffix("'''")))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}
