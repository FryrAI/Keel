use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

use crate::queries;
use crate::resolver::{
    Definition, Import, ParseResult, Reference, ReferenceKind,
};
use keel_core::types::NodeKind;

pub struct TreeSitterParser {
    parser: Parser,
}

impl TreeSitterParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    pub fn parse(&mut self, lang_name: &str, source: &[u8]) -> Result<Tree, ParseError> {
        let lang = language_for_name(lang_name)?;
        self.parser
            .set_language(&lang)
            .map_err(|e| ParseError::Language(format!("{e}")))?;
        self.parser
            .parse(source, None)
            .ok_or(ParseError::ParseFailed)
    }

    pub fn parse_file(
        &mut self,
        lang_name: &str,
        path: &Path,
        source: &str,
    ) -> Result<ParseResult, ParseError> {
        let lang = language_for_name(lang_name)?;
        let query = queries::query_for_language(&lang, lang_name)
            .map_err(ParseError::Query)?;
        self.parser
            .set_language(&lang)
            .map_err(|e| ParseError::Language(format!("{e}")))?;
        let tree = self
            .parser
            .parse(source.as_bytes(), None)
            .ok_or(ParseError::ParseFailed)?;

        let file_path = path.to_string_lossy().to_string();
        let bytes = source.as_bytes();
        let root = tree.root_node();

        let mut definitions = extract_definitions(&query, root, bytes, &file_path);
        let references = extract_references(&query, root, bytes, &file_path);
        let imports = extract_imports(&query, root, bytes, &file_path);

        // Auto-create a Module node for each parsed file
        let line_count = source.lines().count().max(1) as u32;
        let module_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone());
        definitions.insert(
            0,
            Definition {
                name: module_name,
                kind: NodeKind::Module,
                signature: String::new(),
                file_path: file_path.clone(),
                line_start: 1,
                line_end: line_count,
                docstring: None,
                is_public: true,
                type_hints_present: false,
                body_text: String::new(),
            },
        );

        Ok(ParseResult {
            definitions,
            references,
            imports,
            external_endpoints: vec![],
        })
    }
}

impl Default for TreeSitterParser {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("language error: {0}")]
    Language(String),
    #[error("query error: {0}")]
    Query(String),
    #[error("parse failed")]
    ParseFailed,
}

fn language_for_name(name: &str) -> Result<Language, ParseError> {
    match name {
        "typescript" | "javascript" => {
            Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        }
        "tsx" => Ok(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "python" => Ok(tree_sitter_python::LANGUAGE.into()),
        "go" => Ok(tree_sitter_go::LANGUAGE.into()),
        "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
        other => Err(ParseError::UnsupportedLanguage(other.to_string())),
    }
}

fn node_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn extract_definitions(
    query: &Query,
    root: tree_sitter::Node<'_>,
    source: &[u8],
    file_path: &str,
) -> Vec<Definition> {
    let mut cursor = QueryCursor::new();
    let mut defs = Vec::new();
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, root, source);

    while let Some(m) = matches.next() {
        let mut name = None;
        let mut kind = None;
        let mut params_text = String::new();
        let mut return_type_text = String::new();
        let mut body_text = String::new();
        let mut line_start = 0u32;
        let mut line_end = 0u32;

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "def.func.name" | "def.method.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Function);
                }
                "def.class.name" | "def.type.name"
                | "def.struct.name" | "def.enum.name"
                | "def.trait.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Class);
                }
                "def.mod.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Module);
                }
                "def.func.params" | "def.method.params" => {
                    params_text = node_text(cap.node, source).to_string();
                }
                "def.func.return_type" | "def.method.return_type" => {
                    return_type_text = node_text(cap.node, source).to_string();
                }
                "def.func.body" | "def.method.body"
                | "def.class.body" | "def.type.body"
                | "def.struct.body" | "def.enum.body"
                | "def.trait.body" | "def.impl.body" => {
                    body_text = node_text(cap.node, source).to_string();
                }
                "def.func" | "def.method" | "def.class"
                | "def.type" | "def.struct" | "def.enum"
                | "def.trait" | "def.impl" | "def.mod"
                | "def.method.parent" | "def.export"
                | "def.method.receiver" | "def.impl.type" => {
                    line_start = cap.node.start_position().row as u32 + 1;
                    line_end = cap.node.end_position().row as u32 + 1;
                }
                _ => {}
            }
        }

        if let (Some(n), Some(k)) = (name, kind) {
            let signature = if return_type_text.is_empty() {
                format!("{n}{params_text}")
            } else {
                format!("{n}{params_text} -> {return_type_text}")
            };
            let has_type_hints = !params_text.is_empty()
                && (params_text.contains(':') || params_text.contains(" int")
                    || params_text.contains(" string") || params_text.contains(" bool"));

            defs.push(Definition {
                name: n,
                kind: k,
                signature,
                file_path: file_path.to_string(),
                line_start,
                line_end,
                docstring: None,
                is_public: true,
                type_hints_present: has_type_hints,
                body_text,
            });
        }
    }
    // Deduplicate: decorated_definition + standalone patterns can both match
    // the same inner node, producing identical entries.
    defs.dedup_by(|a, b| a.name == b.name && a.line_start == b.line_start);
    defs
}

fn extract_references(
    query: &Query,
    root: tree_sitter::Node<'_>,
    source: &[u8],
    file_path: &str,
) -> Vec<Reference> {
    let mut cursor = QueryCursor::new();
    let mut refs = Vec::new();
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, root, source);

    while let Some(m) = matches.next() {
        let mut call_name = None;
        let mut receiver = None;
        let mut line = 0u32;
        let mut is_call = false;

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "ref.call.name" => {
                    call_name = Some(node_text(cap.node, source).to_string());
                    is_call = true;
                }
                "ref.call.receiver" => {
                    receiver = Some(node_text(cap.node, source).to_string());
                }
                "ref.call" => {
                    line = cap.node.start_position().row as u32 + 1;
                }
                _ => {}
            }
        }

        if let Some(n) = call_name {
            if is_call {
                // For qualified calls (e.g. fmt.Println, Vec::new), include the qualifier
                let qualified_name = match &receiver {
                    Some(recv) if !recv.is_empty() => {
                        // Go uses dot separator, Rust uses ::
                        if recv.contains("::") || n.contains("::") {
                            format!("{recv}::{n}")
                        } else {
                            format!("{recv}.{n}")
                        }
                    }
                    _ => n.clone(),
                };
                refs.push(Reference {
                    name: qualified_name,
                    file_path: file_path.to_string(),
                    line,
                    kind: ReferenceKind::Call,
                    resolved_to: None,
                });
            }
        }
    }
    refs
}

fn extract_imports(
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
            else if let (Some(brace_start), Some(brace_end)) =
                (src.find('{'), src.rfind('}'))
            {
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
                            imported_names
                                .push(item[as_pos + 4..].trim().to_string());
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
            // e.g. "crate::store::GraphStore" → imported_names = ["GraphStore"]
            if imported_names.is_empty() && !is_wildcard && src.contains("::") {
                if let Some(last) = src.rsplit("::").next() {
                    if !last.is_empty() {
                        imported_names.push(last.to_string());
                    }
                }
            }
            // For Go imports without explicit names, extract the package alias
            // e.g. "github.com/spf13/cobra" → imported_names = ["cobra"]
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
        if let Some(existing) = deduped.iter_mut().find(|e| e.source == imp.source && e.line == imp.line) {
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

pub fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("tsx"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

