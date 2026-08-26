mod body_shape;
mod complexity;
mod contexts;
mod docstrings;
mod imports;
#[path = "../supplemental.rs"]
mod supplemental;

pub use supplemental::SupplementalResolver;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree};

use crate::queries;
use crate::resolver::{Definition, ParseResult, Reference, ReferenceKind};
use contexts::{definition_contexts, has_keep_marker};
use keel_core::types::NodeKind;

const BASH_QUERY: &str = r#"
(function_definition name: (word) @def.func.name body: (_) @def.func.body) @def.func
(command name: (command_name (word) @ref.call.name)) @ref.call
"#;

pub struct TreeSitterParser {
    parser: Parser,
    /// Names a string literal may resolve to — the boundary index's key set
    /// (e.g. every `baml_src/*.baml` function name). Empty by default, which
    /// disables literal references entirely: a repo with no boundary surface
    /// pays nothing but the query match itself.
    ///
    /// Shared (`Arc`) because one key set is installed on every language
    /// resolver of a run; it is set once, before the first parse.
    boundary_literals: Arc<HashSet<String>>,
}

impl TreeSitterParser {
    /// Creates a new tree-sitter parser instance.
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            boundary_literals: Arc::new(HashSet::new()),
        }
    }

    /// Install the boundary-name key set used to filter string literals.
    ///
    /// A captured literal is kept only when its text exactly equals one of
    /// these names; everything else is dropped before it reaches the reference
    /// vector. Must be called before parsing, since results are cached per
    /// resolver.
    pub fn set_boundary_literals(&mut self, keys: Arc<HashSet<String>>) {
        self.boundary_literals = keys;
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
        let bash_query;
        let query = if lang_name == "bash" {
            bash_query = Query::new(&lang, BASH_QUERY)
                .map_err(|e| ParseError::Query(format!("query compilation error: {e}")))?;
            &bash_query
        } else {
            queries::query_for_language(&lang, lang_name).map_err(ParseError::Query)?
        };
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

        let definitions = extract_definitions(query, root, bytes, &file_path, lang_name);
        let references =
            extract_references(query, root, bytes, &file_path, &self.boundary_literals);
        let imports = imports::extract_imports(query, root, bytes, &file_path);

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
        "bash" => Ok(tree_sitter_bash::LANGUAGE.into()),
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
    lang: &str,
) -> Vec<Definition> {
    let mut cursor = QueryCursor::new();
    let mut defs = Vec::new();
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, root, source);
    // Split once for the `keel:keep` marker scan below rather than per
    // definition — the file's line count doesn't change during extraction.
    let lines: Vec<&str> = std::str::from_utf8(source).unwrap_or("").lines().collect();

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
        let mut capture_was_macro = false;

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
                    capture_was_macro = true;
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

            // One ancestor walk yields all four context flags.
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
                // Tier 1 default; Go's Tier-2 pass is the only place that sets
                // this true (init/main/TestMain). Other languages have no
                // auto-invoked entrypoint names to mark here.
                is_auto_invoked: false,
                is_decorated: contexts.is_decorated,
                has_keep_marker: has_keep_marker(&lines, line_start),
                // Only `macro_rules!` sets this; the node stays a Function.
                is_macro: capture_was_macro,
                // No body captured (module definitions) means nothing to walk:
                // one decision-free path.
                complexity: body_node.map_or(1, |n| complexity::compute(n, lang)),
                is_trivial_wrapper_body: body_shape::is_trivial_wrapper_body(
                    body_node, lang, source,
                ),
            });
        }
    }
    // Deduplicate: decorated_definition + standalone patterns can both match
    // the same inner node, producing identical entries.
    defs.dedup_by(|a, b| a.name == b.name && a.line_start == b.line_start);
    defs
}

/// Strip the surrounding quotes from a captured string literal's raw text.
///
/// Works for every grammar keel parses: the `string`/`string_literal` node text
/// includes its delimiters, and trimming quote characters from both ends is
/// enough for the plain literals this is applied to. Prefixed forms (Rust
/// `b"x"`, Python `f"{x}"`) simply do not match a boundary name afterwards, so
/// they need no special case.
fn unquote_literal(raw: &str) -> &str {
    raw.trim_matches(|c| c == '"' || c == '\'')
}

fn extract_references(
    query: &Query,
    root: tree_sitter::Node<'_>,
    source: &[u8],
    file_path: &str,
    boundary_literals: &HashSet<String>,
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
        // The whole call-expression node, kept so the argument count can be
        // read off its `arguments` field. Macro invocations never set it —
        // their token tree is not an argument list.
        let mut call_node: Option<tree_sitter::Node<'_>> = None;
        // Functions named as values rather than invoked. Collected separately
        // from calls so they never reach edge-building or E005 arity checks.
        let mut value_names: Vec<(String, u32)> = Vec::new();
        // String literals naming a known boundary symbol (see
        // `ReferenceKind::Literal`). Kept apart from `value_names` because they
        // resolve against the boundary index alone.
        let mut literal_names: Vec<(String, u32)> = Vec::new();

        for cap in m.captures {
            let cap_name = capture_names[cap.index as usize];
            match cap_name {
                "ref.value.name" => {
                    value_names.push((
                        node_text(cap.node, source).to_string(),
                        cap.node.start_position().row as u32 + 1,
                    ));
                }
                "ref.jsx.name" => {
                    // JSX element name (TSX grammar only — see
                    // queries/typescript_jsx.scm). Intrinsic HTML elements
                    // (`<div>`) start lowercase and must NOT count as a
                    // reference; tree-sitter `#match?` predicates are not
                    // evaluated by this walker, so the filter lives here.
                    let text = node_text(cap.node, source);
                    if text.starts_with(|c: char| c.is_uppercase()) {
                        value_names
                            .push((text.to_string(), cap.node.start_position().row as u32 + 1));
                    }
                }
                "ref.attr.name" => {
                    // `"default_true"` — the string literal keeps its quotes.
                    let name = unquote_literal(node_text(cap.node, source));
                    if !name.is_empty() {
                        value_names
                            .push((name.to_string(), cap.node.start_position().row as u32 + 1));
                    }
                }
                "ref.literal.name" => {
                    // A dispatch key in call-argument / match-arm / object-key
                    // position. Only literals naming a symbol the boundary
                    // index already knows survive — everything else (every
                    // other string in the file) is dropped right here, so the
                    // reference vector never grows by free text. With no
                    // boundary surface the set is empty and this is one
                    // `is_empty` check.
                    if !boundary_literals.is_empty() {
                        let name = unquote_literal(node_text(cap.node, source));
                        if boundary_literals.contains(name) {
                            literal_names
                                .push((name.to_string(), cap.node.start_position().row as u32 + 1));
                        }
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
                    call_node = Some(cap.node);
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
                call_arity: None,
            });
        }

        for (name, literal_line) in literal_names {
            refs.push(Reference {
                name,
                file_path: file_path.to_string(),
                line: literal_line,
                kind: ReferenceKind::Literal,
                resolved_to: None,
                call_arity: None,
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
                    call_arity: call_node.and_then(call_argument_count),
                });
            }
        }
    }
    refs
}

/// Count the arguments a call expression actually writes, reading the
/// `arguments` field every grammar keel parses puts on its call node.
///
/// Returns `None` when the count is not knowable from syntax alone: no
/// argument list at all (macro invocations), or a splat/spread argument
/// (`f(*args)`, `f(...rest)`) that expands to an unknown number at runtime.
/// Comments inside the argument list are named children in every grammar and
/// must not count as arguments.
fn call_argument_count(call_node: tree_sitter::Node<'_>) -> Option<u32> {
    let args = call_node.child_by_field_name("arguments")?;
    // The `arguments` field is not always an argument list: Python hangs a
    // bare `generator_expression` there (`total(x for x in xs)`) and
    // TypeScript a `template_string` for tagged templates (`` sql`...` ``).
    // Their named children are comprehension/template internals, not
    // arguments — counting them would hand E005 a wrong arity.
    if !matches!(args.kind(), "arguments" | "argument_list") {
        return None;
    }
    let mut count = 0u32;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        match child.kind() {
            "comment" | "line_comment" | "block_comment" => {}
            "list_splat" | "dictionary_splat" | "spread_element" | "variadic_argument" => {
                return None
            }
            _ => count += 1,
        }
    }
    Some(count)
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
        "typ" => Some("typst"),
        "astro" => Some("astro"),
        "sh" | "bash" | "bats" => Some("bash"),
        "sql" => Some("sql"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_context;

#[cfg(test)]
mod tests_decorators;

#[cfg(test)]
mod tests_value_captures;

#[cfg(test)]
mod tests_literal_captures;

#[cfg(test)]
mod tests_import_names;

#[cfg(test)]
mod tests_call_arity;

#[cfg(test)]
mod tests_complexity;
