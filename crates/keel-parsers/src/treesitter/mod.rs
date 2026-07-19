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

/// The three ancestry-derived flags a definition carries.
#[derive(Default)]
struct DefContexts {
    /// Rust only: inside a `#[cfg(test)]` module or a `#[test]` function.
    in_test: bool,
    /// On a trait/interface contract surface (Rust trait or trait impl, TS
    /// interface or `implements` class).
    in_trait: bool,
    /// An associated item — addressed through its type, so bare-name
    /// collisions across unrelated types are idiomatic and W002 skips it.
    is_associated: bool,
}

/// Node kinds that introduce a new function scope, per supported grammar.
///
/// Used to BOUND the associated-item walk: a `fn` nested inside an `impl`
/// method is a local helper, not an associated item, and must not inherit the
/// enclosing `impl`'s associated-ness.
fn is_function_scope(kind: &str) -> bool {
    matches!(
        kind,
        // rust
        "function_item"
            // typescript / javascript
            | "function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            // python
            | "function_definition"
            // go (`function_declaration` shared with TS above)
            | "method_declaration"
    )
}

/// Compute all three ancestry flags for a definition in ONE walk up the tree.
///
/// These were three separate full walks over the same ancestor chain. They stop
/// on different things, but nothing stops them from sharing the traversal.
///
/// Note the deliberate asymmetry: only the associated-item flag stops at a
/// function scope. A nested helper inside a `#[cfg(test)] mod` is still test
/// code, and one inside a trait impl is still (transitively) trait-context
/// code — but it is emphatically not itself an associated item.
fn definition_contexts(node: tree_sitter::Node<'_>, lang: &str, source: &[u8]) -> DefContexts {
    let mut ctx = DefContexts::default();
    let mut trait_decided = false;
    let mut assoc_decided = false;
    let is_ts = is_typescript_family(lang);

    let mut current = Some(node);
    let mut is_self = true;
    while let Some(n) = current {
        let kind = n.kind();

        // Test context (Rust only) — starts at the node ITSELF, since a `#[test]`
        // attribute annotates the function being classified.
        if lang == "rust"
            && !ctx.in_test
            && matches!(kind, "function_item" | "mod_item")
            && preceding_attrs_mark_test(n, source)
        {
            ctx.in_test = true;
        }

        // The remaining two flags are about ANCESTRY, so skip the node itself.
        if !is_self {
            if !trait_decided {
                match (lang, kind) {
                    ("rust", "trait_item") => {
                        ctx.in_trait = true;
                        trait_decided = true;
                    }
                    // Both Rust impl forms are `impl_item`; only `impl T for S`
                    // carries a `trait` field. Verified against the grammar.
                    ("rust", "impl_item") => {
                        ctx.in_trait = n.child_by_field_name("trait").is_some();
                        trait_decided = true;
                    }
                    _ if is_ts && kind == "interface_declaration" => {
                        ctx.in_trait = true;
                        trait_decided = true;
                    }
                    // A bare class, or one that only `extends`, is ordinary
                    // owned code; only `implements` is a contract surface.
                    _ if is_ts && matches!(kind, "class_declaration" | "class") => {
                        ctx.in_trait = has_implements_clause(n);
                        trait_decided = true;
                    }
                    _ => {}
                }
            }

            if !assoc_decided {
                if is_function_scope(kind) {
                    // A function scope encloses us before any type body does:
                    // we are a local helper, not an associated item.
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
        }

        is_self = false;
        current = n.parent();
    }
    ctx
}

/// True when a TS class node carries an `implements` clause.
///
/// tree-sitter-typescript hangs `implements Y` off the `class_heritage` child
/// as an `implements_clause`, while `extends D` yields a `class_heritage` with
/// no such clause.
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
        let mut capture_was_method = false;

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "def.func.name" | "def.method.name" => {
                    name = Some(node_text(cap.node, source).to_string());
                    kind = Some(NodeKind::Function);
                    capture_was_method = cap_name == "def.method.name";
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

            // One ancestor walk yields all three context flags.
            let contexts = def_node
                .map(|node| definition_contexts(node, lang, source))
                .unwrap_or_default();
            // Go methods carry no class ancestry; the capture kind marks them.
            let is_go_method = lang == "go" && capture_was_method;

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
                in_test_context: contexts.in_test,
                in_trait_context: contexts.in_trait,
                is_associated: is_go_method || contexts.is_associated,
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
        // Functions named as values rather than invoked. Collected separately
        // from calls so they never reach edge-building or E005 arity checks.
        let mut value_names: Vec<(String, u32)> = Vec::new();

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "ref.value.name" => {
                    value_names.push((
                        node_text(cap.node, source).to_string(),
                        cap.node.start_position().row as u32 + 1,
                    ));
                }
                "ref.attr.name" => {
                    // `"default_true"` — the string literal keeps its quotes.
                    let raw = node_text(cap.node, source);
                    let name = raw.trim_matches(|c| c == '"' || c == '\'');
                    if !name.is_empty() {
                        value_names
                            .push((name.to_string(), cap.node.start_position().row as u32 + 1));
                    }
                }
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

        for (name, value_line) in value_names {
            refs.push(Reference {
                name,
                file_path: file_path.to_string(),
                line: value_line,
                kind: ReferenceKind::Value,
                resolved_to: None,
            });
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
