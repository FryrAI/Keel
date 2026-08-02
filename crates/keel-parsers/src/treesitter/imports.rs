//! Import extraction from tree-sitter query matches.

use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::resolver::Import;

use super::node_text;

/// Extracts import declarations from tree-sitter query matches for all supported languages.
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
    merge_by_statement(imports)
}

/// Go's blank (`_`) and dot (`.`) import markers, which stand in for the whole
/// binding rather than naming one of several.
fn is_marker(names: &[String]) -> bool {
    names.iter().any(|n| n == "_" || n == ".")
}

/// Collapses the several query matches produced by one import *statement* into
/// one [`Import`] carrying the union of their names.
///
/// One statement yields one match per binding: the named-import pattern fires
/// once per `import_specifier`, and the default-import / side-effect patterns
/// match the same statement again. Every one of them reports the same
/// `(source, line)`, so they all describe a single import.
///
/// This used to keep the FIRST match and drop the rest, which silently reduced
/// `import { a, b, c } from './m'` to `["a"]` — for every language, not just
/// TypeScript (`from mod import a, b, c` collapsed the same way). Nothing then
/// recorded that `b` and `c` were in scope, so `resolve_cross_file_call` could
/// not resolve a single reference to them and their definitions read as dead
/// code. It is the root cause behind a SvelteKit `model.ts` reporting nine of
/// its thirteen exports at zero callers while `applyView` — the first name in
/// the route's import list — resolved fine.
///
/// The one case that must NOT be unioned is a Go blank/dot marker: there the
/// specific and the generic pattern describe the same single binding, so the
/// marker replaces the package name instead of joining it.
fn merge_by_statement(imports: Vec<Import>) -> Vec<Import> {
    let mut merged: Vec<Import> = Vec::with_capacity(imports.len());
    for imp in imports {
        let Some(existing) = merged
            .iter_mut()
            .find(|e| e.source == imp.source && e.line == imp.line)
        else {
            merged.push(imp);
            continue;
        };
        if is_marker(&imp.imported_names) {
            *existing = imp;
        } else if !is_marker(&existing.imported_names) {
            for name in imp.imported_names {
                if !existing.imported_names.contains(&name) {
                    existing.imported_names.push(name);
                }
            }
        }
    }
    merged
}
