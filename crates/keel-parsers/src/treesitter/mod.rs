mod docstrings;
mod imports;

use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

use crate::queries;
use crate::resolver::{Definition, ParseResult, Reference, ReferenceKind};
use keel_core::types::NodeKind;

pub struct TreeSitterParser {
    parser: Parser,
}

impl TreeSitterParser {
    /// Creates a new tree-sitter parser instance.
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
        }
    }

    /// Parses raw source bytes into a tree-sitter syntax tree for the given language.
    pub fn parse(&mut self, lang_name: &str, source: &[u8]) -> Result<Tree, ParseError> {
        let lang = language_for_name(lang_name)?;
        self.parser
            .set_language(&lang)
            .map_err(|e| ParseError::Language(format!("{e}")))?;
        self.parser
            .parse(source, None)
            .ok_or(ParseError::ParseFailed)
    }

    /// Parses a source file and extracts definitions, references, and imports.
    pub fn parse_file(
        &mut self,
        lang_name: &str,
        path: &Path,
        source: &str,
    ) -> Result<ParseResult, ParseError> {
        let lang = language_for_name(lang_name)?;
        let query = queries::query_for_language(&lang, lang_name).map_err(ParseError::Query)?;
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

        let definitions = extract_definitions(&query, root, bytes, &file_path, lang_name);
        let references = extract_references(&query, root, bytes, &file_path);
        let imports = imports::extract_imports(&query, root, bytes, &file_path);

        // The whole-file Module node is owned by `map_passes::first_pass`
        // (path-named, one per file). Emitting a second file-stem-named Module
        // here duplicated every module row and made `find_containing_def`
        // attribute call edges to the file instead of the enclosing function.

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
        "typescript" | "javascript" => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Ok(tree_sitter_typescript::LANGUAGE_TSX.into()),
        // Svelte script blocks are parsed with the TypeScript grammar after the
        // markup is blanked out (see `typescript::svelte`).
        "svelte" => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "python" => Ok(tree_sitter_python::LANGUAGE.into()),
        "go" => Ok(tree_sitter_go::LANGUAGE.into()),
        "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
        other => Err(ParseError::UnsupportedLanguage(other.to_string())),
    }
}

fn node_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// True when a Rust definition node sits in a test context: inside a
/// `#[cfg(test)]`-annotated module, or is itself a `#[test]` / `#[tokio::test]`
/// function. Walks ancestors (tree-sitter gives us the full ancestry here) and
/// inspects the preceding `attribute_item` siblings of each enclosing item.
///
/// The node kinds checked (`function_item`, `mod_item`, `attribute_item`) are
/// Rust-specific, so this returns `false` for every other grammar — callers
/// gate on the language anyway.
fn in_rust_test_context(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        if matches!(n.kind(), "function_item" | "mod_item") && preceding_attrs_mark_test(n, source)
        {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Scans the `attribute_item` siblings immediately preceding `item` for a
/// `#[cfg(test)]`, `#[test]`, or `#[<path>::test]` marker. In tree-sitter-rust,
/// outer attributes are preceding siblings of the item they annotate, not
/// children of it.
fn preceding_attrs_mark_test(item: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut sib = item.prev_sibling();
    while let Some(s) = sib {
        match s.kind() {
            "attribute_item" => {
                let compact: String = node_text(s, source)
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
            // Comments may sit between an attribute and its item — skip them
            // and keep scanning upward; anything else ends the attribute run.
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sib = s.prev_sibling();
    }
    false
}

fn extract_definitions(
    query: &Query,
    root: tree_sitter::Node<'_>,
    source: &[u8],
    file_path: &str,
    lang: &str,
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
        let mut def_node = None;
        let mut body_node = None;

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "def.func.name" | "def.method.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Function);
                }
                "def.class.name" | "def.type.name" | "def.struct.name" | "def.enum.name"
                | "def.trait.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Class);
                }
                "def.macro.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Function); // macro_rules treated as Function kind
                }
                "def.mod.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Module);
                }
                "def.func.params" | "def.method.params" => {
                    params_text = node_text(cap.node, source).to_string();
                }
                "def.func.return_type" | "def.method.return_type" => {
                    // TS annotation nodes include the leading `:` (e.g. `: number`);
                    // strip it (and surrounding space) so the signature renders
                    // `add(...) -> number`, not `-> : number`. Harmless for the
                    // already-bare Rust/Python/Go return types.
                    return_type_text = node_text(cap.node, source)
                        .trim_start_matches(':')
                        .trim()
                        .to_string();
                }
                "def.func.body" | "def.method.body" | "def.class.body" | "def.type.body"
                | "def.struct.body" | "def.enum.body" | "def.trait.body" | "def.impl.body" => {
                    body_text = node_text(cap.node, source).to_string();
                    body_node = Some(cap.node);
                }
                // Primary definition nodes — use for docstring extraction
                "def.func" | "def.method" | "def.class" | "def.type" | "def.struct"
                | "def.enum" | "def.trait" | "def.mod" | "def.macro" => {
                    line_start = cap.node.start_position().row as u32 + 1;
                    line_end = cap.node.end_position().row as u32 + 1;
                    def_node = Some(cap.node);
                }
                // Secondary/parent nodes — only set lines if primary didn't
                "def.impl"
                | "def.trait_impl"
                | "def.method.parent"
                | "def.export"
                | "def.method.receiver"
                | "def.impl.type"
                | "def.trait_impl.trait_name"
                | "def.trait_impl.type_name"
                | "def.trait_impl.body"
                    if def_node.is_none() =>
                {
                    line_start = cap.node.start_position().row as u32 + 1;
                    line_end = cap.node.end_position().row as u32 + 1;
                    def_node = Some(cap.node);
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
            let has_type_hints = !return_type_text.is_empty()
                || (!params_text.is_empty()
                    && (params_text.contains(':')
                        || params_text.contains(" int")
                        || params_text.contains(" string")
                        || params_text.contains(" bool")));

            let docstring =
                def_node.and_then(|node| docstrings::extract_docstring(node, body_node, source));

            let in_test_context =
                lang == "rust" && def_node.is_some_and(|node| in_rust_test_context(node, source));

            defs.push(Definition {
                name: n,
                kind: k,
                signature,
                file_path: file_path.to_string(),
                line_start,
                line_end,
                docstring,
                is_public: true,
                type_hints_present: has_type_hints,
                body_text,
                in_test_context,
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
                "ref.macro_invocation.name" => {
                    // Capture macro invocations as calls with ! suffix
                    call_name = Some(format!("{}!", node_text(cap.node, source)));
                    is_call = true;
                }
                "ref.macro_invocation" => {
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

/// Returns true when `lang` (a [`detect_language`] result) belongs to the
/// TypeScript resolver family — TS/TSX grammars plus extracted Svelte
/// script blocks. The single source of truth for the family collapse.
pub fn is_typescript_family(lang: &str) -> bool {
    matches!(lang, "typescript" | "tsx" | "javascript" | "svelte")
}

/// Detects the programming language from a file's extension.
pub fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("tsx"),
        "svelte" => Some("svelte"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
