use std::collections::HashMap;
use std::path::Path;

/// Per-file symbol information extracted from oxc_semantic analysis.
#[derive(Debug, Clone)]
pub(crate) struct OxcSymbolInfo {
    /// Symbol name -> (is_exported, has_type_annotation)
    pub symbols: HashMap<String, (bool, bool)>,
    /// Re-export mappings: local_name -> (source_module, original_name)
    pub reexports: HashMap<String, (String, String)>,
}

/// Run oxc_semantic analysis on source to build a symbol table.
/// Returns symbol info keyed by name with export/type status.
pub(crate) fn analyze_with_oxc(path: &Path, content: &str) -> OxcSymbolInfo {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();

    let parse_result = OxcParser::new(&allocator, content, source_type).parse();
    if !parse_result.errors.is_empty() {
        return OxcSymbolInfo {
            symbols: HashMap::new(),
            reexports: HashMap::new(),
        };
    }

    let semantic_ret = SemanticBuilder::new().build(&parse_result.program);
    let semantic = semantic_ret.semantic;
    let scopes = semantic.scopes();
    let symbols = semantic.symbols();

    let mut symbol_map = HashMap::new();
    let root_scope = scopes.root_scope_id();

    // Detect exported names from source (no Export flag in SymbolFlags 0.49)
    let exported_names = detect_exported_names(content);

    // For JS files, oxc can't infer type annotations — defer to JSDoc check.
    let is_js = super::helpers::is_js_file(path);

    // Walk top-level bindings in root scope
    for symbol_id in scopes.iter_bindings_in(root_scope) {
        let name = symbols.get_name(symbol_id).to_string();
        let is_exported = exported_names.contains(&name);
        // TS files: oxc parsed it successfully = we have precise type info.
        // JS files: type annotations aren't present; JSDoc pass handles it later.
        let has_type = !is_js;
        symbol_map.insert(name, (is_exported, has_type));
    }

    // Detect re-exports: `export { X } from './module'`
    let reexports = extract_reexports(content);

    OxcSymbolInfo {
        symbols: symbol_map,
        reexports,
    }
}

/// Detect names that appear in `export` declarations in the source.
/// Handles: `export function X`, `export class X`, `export const X`,
/// `export default X`, `export { X, Y }`.
pub(crate) fn detect_exported_names(content: &str) -> std::collections::HashSet<String> {
    use super::helpers::extract_decl_name;

    let mut names = std::collections::HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("export") {
            continue;
        }
        // `export function name` / `export class name` / `export const name`
        let after_export = trimmed.strip_prefix("export").unwrap().trim();
        if after_export.starts_with("default ") {
            let rest = after_export.strip_prefix("default").unwrap().trim();
            if let Some(name) = extract_decl_name(rest) {
                names.insert(name);
            }
            continue;
        }
        if let Some(name) = extract_decl_name(after_export) {
            names.insert(name);
            continue;
        }
        // `export { X, Y }` or `export { X as Z }`
        if let Some(brace_start) = trimmed.find('{') {
            if let Some(brace_end) = trimmed.find('}') {
                let inner = &trimmed[brace_start + 1..brace_end];
                for entry in inner.split(',') {
                    let parts: Vec<&str> = entry.trim().split(" as ").collect();
                    let original = parts[0].trim();
                    if !original.is_empty() {
                        names.insert(original.to_string());
                    }
                }
            }
        }
    }
    names
}

/// Extract re-exports from source text.
/// Parses patterns like: `export { Foo, Bar } from './module'`
pub(crate) fn extract_reexports(content: &str) -> HashMap<String, (String, String)> {
    use super::helpers::extract_string_literal;

    let mut reexports = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("export") || !trimmed.contains("from") {
            continue;
        }
        // Simple pattern: export { names } from 'source'
        if let Some(brace_start) = trimmed.find('{') {
            if let Some(brace_end) = trimmed.find('}') {
                let names_part = &trimmed[brace_start + 1..brace_end];
                let from_idx = trimmed.find("from").unwrap_or(trimmed.len());
                let source_part = &trimmed[from_idx..];
                let source = extract_string_literal(source_part);
                if let Some(src) = source {
                    for name_entry in names_part.split(',') {
                        let parts: Vec<&str> = name_entry.trim().split(" as ").collect();
                        let original = parts[0].trim().to_string();
                        let local = if parts.len() > 1 {
                            parts[1].trim().to_string()
                        } else {
                            original.clone()
                        };
                        reexports.insert(local, (src.clone(), original));
                    }
                }
            }
        }
        // export * from './module' — can't map individual names
    }
    reexports
}
