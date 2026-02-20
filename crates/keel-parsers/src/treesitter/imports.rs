//! Import extraction from tree-sitter query matches.

use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::resolver::Import;

use super::node_text;

pub(super) fn extract_imports(
    query: &Query,
    root: tree_sitter::Node<'_>,
    source: &[u8],
    file_path: &str,
) -> Vec<Import> {
    let mut cursor = QueryCursor::new();
    let mut imports = Vec::new();
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, root, source);

    while let Some(m) = matches.next() {
        let mut source_path = None;
        let mut imported_names = Vec::new();
        let mut line = 0u32;

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "ref.import.source" => {
                    let raw = node_text(cap.node, source);
                    source_path = Some(raw.trim_matches('"').trim_matches('\'').to_string());
                }
                "ref.import.name" => {
                    imported_names.push(node_text(cap.node, source).to_string());
                }
                "ref.import.blank" => {
                    imported_names.push("_".to_string());
                }
                "ref.import.dot" => {
                    imported_names.push(".".to_string());
                }
                "ref.import.star" => {
                    imported_names.push("*".to_string());
                }
                "ref.import" => {
                    line = cap.node.start_position().row as u32 + 1;
                }
                "ref.use.path" => {
                    source_path = Some(node_text(cap.node, source).to_string());
                }
                "ref.use" => {
                    line = cap.node.start_position().row as u32 + 1;
                }
                _ => {}
            }
        }

        if let Some(raw_src) = source_path {
            let mut src = raw_src;
            let is_relative = src.starts_with('.')
                || src.starts_with("./")
                || src.starts_with("../")
                || src.starts_with("crate::")
                || src.starts_with("super::");

            // Handle Rust use statement special syntax before default extraction
            let mut is_wildcard = false;
            // 1. Alias: "crate::module::Name as Alias"
            if src.contains(" as ") && !src.contains('{') {
                if let Some(as_pos) = src.rfind(" as ") {
                    let alias = src[as_pos + 4..].trim().to_string();
                    src = src[..as_pos].trim().to_string();
                    imported_names.push(alias);
                }
            }
            // 2. Use list: "crate::module::{A, B, self}"
            else if let (Some(brace_start), Some(brace_end)) = (src.find('{'), src.rfind('}')) {
                let base = src[..brace_start].trim_end_matches("::").to_string();
                let items_str = &src[brace_start + 1..brace_end];
                for item in items_str.split(',') {
                    let item = item.trim();
                    if item == "self" {
                        // self refers to the module itself
                        if let Some(module_name) = base.rsplit("::").next() {
                            imported_names.push(module_name.to_string());
                        }
                    } else if item.contains(" as ") {
                        if let Some(as_pos) = item.rfind(" as ") {
                            imported_names.push(item[as_pos + 4..].trim().to_string());
                        }
                    } else if !item.is_empty() {
                        imported_names.push(item.to_string());
                    }
                }
                src = base;
            }
            // 3. Wildcard: "crate::module::*"
            else if src.ends_with("::*") {
                src = src[..src.len() - 3].to_string();
                is_wildcard = true;
                // imported_names stays empty for wildcard
            }

            // Fallback: For simple Rust use paths, extract the last segment
            // e.g. "crate::store::GraphStore" -> imported_names = ["GraphStore"]
            if imported_names.is_empty() && !is_wildcard && src.contains("::") {
                if let Some(last) = src.rsplit("::").next() {
                    if !last.is_empty() {
                        imported_names.push(last.to_string());
                    }
                }
            }
            // For Go imports without explicit names, extract the package alias
            // e.g. "github.com/spf13/cobra" -> imported_names = ["cobra"]
            if imported_names.is_empty() && src.contains('/') && !src.starts_with('.') {
                if let Some(last) = src.rsplit('/').next() {
                    if !last.is_empty() {
                        imported_names.push(last.to_string());
                    }
                }
            }
            imports.push(Import {
                source: src,
                imported_names,
                file_path: file_path.to_string(),
                line,
                is_relative,
            });
        }
    }
    // Deduplicate: blank/dot import queries may match the same import_spec as the
    // basic pattern. Keep the more specific entry (the one with "_" or "." markers)
    // over the plain one for the same (source, line).
    let mut deduped: Vec<Import> = Vec::with_capacity(imports.len());
    for imp in imports {
        let special = imp.imported_names.iter().any(|n| n == "_" || n == ".");
        if let Some(existing) = deduped
            .iter_mut()
            .find(|e| e.source == imp.source && e.line == imp.line)
        {
            // Replace plain entry with the more specific blank/dot entry
            if special {
                *existing = imp;
            }
        } else {
            deduped.push(imp);
        }
    }
    deduped
}
