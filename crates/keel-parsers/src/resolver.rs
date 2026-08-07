use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use keel_core::types::{ExternalEndpoint, NodeKind};

// ---------------------------------------------------------------------------
// FROZEN CONTRACT -- LanguageResolver trait
// Agent A owns this interface. Agent B consumes it.
// Do NOT modify the trait signature without coordinating across all agents.
// ---------------------------------------------------------------------------

/// The core abstraction every language-specific parser must implement.
///
/// Each resolver is responsible for:
/// - Parsing source files into definitions, references, and imports (Tier 1).
/// - Resolving call-site edges with a confidence score (Tier 2).
///
/// Implementors must be `Send + Sync` so they can be shared across threads
/// for parallel parsing.
pub trait LanguageResolver: Send + Sync {
    /// Returns the canonical language name (e.g. "typescript", "python").
    fn language(&self) -> &str;

    /// Parse a single file and return all structural information.
    fn parse_file(&self, path: &Path, content: &str) -> ParseResult;

    /// Return definitions found in `file`. May re-parse or use cached data.
    fn resolve_definitions(&self, file: &Path) -> Vec<Definition>;

    /// Return references (calls, imports, type-refs) found in `file`.
    fn resolve_references(&self, file: &Path) -> Vec<Reference>;

    /// Attempt to resolve a call-site to a concrete target.
    /// Returns `None` when resolution is ambiguous or unsupported.
    fn resolve_call_edge(&self, call_site: &CallSite) -> Option<ResolvedEdge>;

    /// Returns the file extensions this resolver handles (without leading dot).
    /// Default implementation returns an empty slice.
    fn supported_extensions(&self) -> &[&str] {
        &[]
    }
}

// ---------------------------------------------------------------------------
// Types returned by LanguageResolver methods
// ---------------------------------------------------------------------------

/// Complete parse output for a single source file.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
    pub imports: Vec<Import>,
    pub external_endpoints: Vec<ExternalEndpoint>,
}

/// A definition (function, class, module) extracted from source.
#[derive(Debug, Clone)]
pub struct Definition {
    /// Simple name of the symbol (e.g. "handleRequest").
    pub name: String,
    /// What kind of graph node this maps to.
    pub kind: NodeKind,
    /// Canonical signature string used for hash computation.
    pub signature: String,
    /// Absolute or repo-relative file path.
    pub file_path: String,
    /// First line of the definition (1-based).
    pub line_start: u32,
    /// Last line of the definition (1-based, inclusive).
    pub line_end: u32,
    /// Leading doc-comment, if any.
    pub docstring: Option<String>,
    /// Whether the symbol is exported / public.
    pub is_public: bool,
    /// Whether all parameters and return value carry type annotations.
    pub type_hints_present: bool,
    /// Raw body text (used for hash computation after AST normalization).
    pub body_text: String,
    /// True when this definition was extracted from a test context — a Rust
    /// `#[cfg(test)]` module, or a `#[test]` / `#[tokio::test]`-attributed
    /// function. Test-context symbols are invoked by the harness, not by
    /// production code, so W005 (dead code), E002 (type hints), and E003
    /// (docstrings) skip them to avoid vacuous "no callers" noise.
    pub in_test_context: bool,
    /// True when this definition sits on an abstract contract surface rather
    /// than being ordinary owned code:
    ///
    /// - **Rust** — a method declared in a `trait` block, or defined in a trait
    ///   implementation (`impl Trait for Type`). An inherent `impl Type` block
    ///   is NOT a trait context; same-named inherent methods still get flagged.
    /// - **TypeScript** — a method declared in an `interface`, or defined in a
    ///   `class X implements Y` body. A plain class, or one that only
    ///   `extends` a base, is NOT an interface context.
    ///
    /// Trait methods are *required* by the language to share their names and
    /// often their shapes across every implementor, and are reached through
    /// static/dynamic dispatch that the call graph resolves to the trait
    /// declaration rather than to each implementor. W002 (duplicate name),
    /// W005 (dead code), and W006 (duplicate implementation) therefore skip
    /// them — renaming or deleting one is not a legal fix.
    ///
    /// Python definitions always carry `false`: Python has no static
    /// interface declaration. Go methods carry `false` UNLESS the method's
    /// receiver type satisfies a same-file interface (Go's Tier-2 pass sets
    /// `true` there — see `go::mod`) — Go interfaces are satisfied
    /// structurally, so there is no syntactic marker on the method itself to
    /// key off; the parser has to cross-reference the interface's method set.
    pub in_trait_context: bool,
    /// True when this definition is an associated item — a method or associated
    /// function inside a Rust `impl`/`trait` block or a TS/Python class body.
    /// Associated items are addressed as `Type::name` / `obj.name`, so a shared
    /// bare name across unrelated types is idiomatic, not ambiguous — W002
    /// skips them. Go methods (receiver funcs) count too.
    pub is_associated: bool,
    /// True when this definition is invoked by a language runtime or test
    /// harness rather than by an explicit call in the codebase — so "no
    /// callers" is vacuous and W005 must not flag it. Set per-language in the
    /// Tier-2 pass: Go's `init`/`main`/`TestMain`. Lets the enforcement layer's
    /// entrypoint list hold only genuinely universal names instead of accreting
    /// a language arm per runtime convention. Other languages carry `false`.
    pub is_auto_invoked: bool,
    /// True when this definition sits directly under a decorator — a Python
    /// `@register("evt")` / `@app.route(...)` / `@pytest.fixture`-wrapped
    /// function (a `decorated_definition` node in tree-sitter-python).
    /// Decorator syntax hands the function to the decorator rather than
    /// leaving it called by name — a framework holds the reference, so "no
    /// callers" is an artifact of the analysis, not dead code. W005 skips it;
    /// E002 (type hints) and E003 (docstrings) still apply. Python-only for
    /// now (TS decorators cannot wrap a plain function); other languages
    /// always carry `false`.
    pub is_decorated: bool,
    /// True when the substring `keel:keep` appears on this definition's own
    /// line or the line immediately above it (`# keel:keep`, `// keel:keep`,
    /// `/* keel:keep */` all match — the scan is a dumb substring check, not
    /// comment-aware). The language-agnostic per-symbol escape hatch for
    /// statically-invisible dynamic dispatch (`globals()[name]()`, `getattr`,
    /// a handler table keyed by string) that no exemption rule can see
    /// through. W005 skips it.
    pub has_keep_marker: bool,
    /// True when this definition is a macro rather than a function — today
    /// only a Rust `macro_rules! name` block. The node keeps
    /// `NodeKind::Function` so nothing downstream changes shape; this flag
    /// exists so macro *invocations* (`name!(...)`) resolve to macro
    /// definitions and never to a same-named function. Deliberately
    /// in-memory only: its sole consumer is the Rust resolver, which reads
    /// the parse cache rather than the graph, so there is no `nodes.is_macro`
    /// column and no schema bump. Other languages always carry `false`.
    pub is_macro: bool,
    /// McCabe cyclomatic complexity of this definition's body: 1 + the number
    /// of decision points (branches, loops, short-circuiting boolean
    /// operators, catch/except arms, match/case arms) in its subtree, not
    /// counting nested named function/method definitions — they get their own
    /// count. 1 for modules, which capture no body. Deliberately absent from
    /// [`Definition::hash`]: it is derived from `body_text`, which already
    /// drives the hash.
    pub complexity: u32,
    /// True when this definition's body is exactly one statement that is a
    /// call expression — a bare call, `return call(...)`, or (TS/JS) a
    /// concise-arrow body that IS the call — modulo a leading docstring
    /// (Python) or `pass` (Python). Detected once at parse time in
    /// `treesitter::body_shape` because it needs the real AST node, not the
    /// flattened `body_text`. Consumed by `keel audit`'s `trivial_wrapper`
    /// smell, combined there with the stored graph's caller count.
    pub is_trivial_wrapper_body: bool,
}

impl Definition {
    /// Body text normalized for node-hash computation (issue #36).
    ///
    /// Strips comments and collapses whitespace in a language-aware way (see
    /// [`keel_core::hash::normalize_body_for_hash`]) so that reformatting and
    /// non-doc comment edits leave the hash stable, while any token-level code
    /// change moves it. The language is derived from the definition's file
    /// extension, so `keel map` and `keel compile` normalize identically.
    pub fn body_for_hash(&self) -> String {
        let lang = crate::treesitter::detect_language(Path::new(&self.file_path)).unwrap_or("");
        keel_core::hash::normalize_body_for_hash(&self.body_text, lang)
    }

    /// The canonical content hash of this definition: `compute_hash` over the
    /// signature, normalized body, and docstring — exactly the triple
    /// `keel map` stores for a non-colliding symbol. Single source of truth
    /// for the several call sites that previously inlined this expression.
    pub fn hash(&self) -> String {
        keel_core::hash::compute_hash(
            &self.signature,
            &self.body_for_hash(),
            self.docstring.as_deref().unwrap_or(""),
        )
    }

    /// The disambiguated content hash, salted with `file_path` — the identity
    /// `keel map` falls back to when a symbol's base [`hash`](Self::hash)
    /// collides with a different node.
    pub fn hash_disambiguated(&self, file_path: &str) -> String {
        keel_core::hash::compute_hash_disambiguated(
            &self.signature,
            &self.body_for_hash(),
            self.docstring.as_deref().unwrap_or(""),
            file_path,
        )
    }
}

/// The flavour of a reference occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A function/method call.
    Call,
    /// An import statement reference.
    Import,
    /// A type annotation or type-level reference.
    TypeRef,
    /// A function named as a *value* rather than invoked — passed as an
    /// argument (`.map(render_file)`, `get(health)`,
    /// `registerCommand("keel.compile", cmdCompile)`) or named in an attribute
    /// string (`#[serde(default = "default_true")]`).
    ///
    /// These are real usages, but they are not call sites: they carry no
    /// argument list and must never produce a `calls` edge or feed E005 arity
    /// checking. W005 dead-code analysis consumes them so a function that is
    /// only ever handed around as a value is not reported as uncalled.
    Value,
    /// A name recovered from framework *template markup* by a lexical scan —
    /// today, an imported binding named inside a Svelte `{ ... }` expression
    /// (`{@const pct = completenessPct(v)}`, `<Panel onDone={refresh} />`).
    ///
    /// The markup is never parsed, so this is a whole-word text match with no
    /// syntax behind it: it proves the binding is used, and nothing more. Like
    /// `Value` it becomes a `uses` edge, but at a lower confidence
    /// (`keel_core::confidence::TEMPLATE_LEXICAL`) and under its own
    /// `tier1_template` resolution tier, so a lexical hit is never mistaken for
    /// a parsed call site.
    Template,
    /// A *string literal* whose text is the name of a known boundary symbol —
    /// a cross-language dispatch key, e.g. the `"PlanBerichtSection"` in
    /// `run_baml("PlanBerichtSection", input)` naming a `baml_src/*.baml`
    /// function.
    ///
    /// Only literals in three syntactic positions are considered (call
    /// argument, `match`-arm pattern, object/map key), and only those whose
    /// text *exactly* equals a name already in the boundary index survive: a
    /// literal that matches nothing known is dropped inside the parser and
    /// never reaches this vector, so the graph gains no free-text nodes.
    ///
    /// Like `Value` it becomes a `uses` edge — at the producing boundary
    /// provider's own confidence, under the `tier1_boundary_literal` tier. A
    /// string is not a call site: it carries no argument list, so it must never
    /// reach E001/E004/E005 or the fix planner.
    Literal,
}

/// A reference (usage) of a symbol within a file.
#[derive(Debug, Clone)]
pub struct Reference {
    /// Name of the referenced symbol.
    pub name: String,
    /// File where the reference occurs.
    pub file_path: String,
    /// Line number of the reference (1-based).
    pub line: u32,
    /// What kind of reference this is.
    pub kind: ReferenceKind,
    /// If already resolved, the hash/id of the target definition.
    pub resolved_to: Option<String>,
    /// How many arguments the call site actually writes, when that is knowable
    /// from syntax alone. `None` for non-call references, for call-shaped
    /// references with no argument list (attribute macros, template markup
    /// hits), and for argument lists containing a splat/spread — E005 arity
    /// checking must skip all of those rather than compare against a guess.
    pub call_arity: Option<u32>,
}

/// An import statement extracted from source.
#[derive(Debug, Clone)]
pub struct Import {
    /// The module specifier / source path.
    pub source: String,
    /// Individual names brought into scope (empty for wildcard/namespace imports).
    pub imported_names: Vec<String>,
    /// File containing the import.
    pub file_path: String,
    /// Line number (1-based).
    pub line: u32,
    /// Whether this is a relative import (e.g. `./foo` or `from .bar`).
    pub is_relative: bool,
}

/// A call site that needs resolution.
#[derive(Debug, Clone)]
pub struct CallSite {
    /// File containing the call.
    pub file_path: String,
    /// Line number of the call (1-based).
    pub line: u32,
    /// Name of the function/method being called.
    pub callee_name: String,
    /// Receiver expression, if this is a method call (e.g. "self", "obj").
    pub receiver: Option<String>,
}

/// The result of successfully resolving a call edge.
#[derive(Debug, Clone)]
pub struct ResolvedEdge {
    /// File containing the target definition.
    pub target_file: String,
    /// Name of the target definition.
    pub target_name: String,
    /// Resolution confidence (0.0 = guess, 1.0 = certain).
    /// Low-confidence edges produce WARNINGs, not ERRORs.
    pub confidence: f64,
    /// Which resolution tier produced this edge (e.g. "tier1", "tier2_oxc", "tier2_ty").
    pub resolution_tier: String,
}

/// Aggregated index for a single file -- used by the incremental pipeline
/// to decide whether a file needs re-parsing.
#[derive(Debug, Clone)]
pub struct FileIndex {
    /// Absolute or repo-relative file path.
    pub file_path: String,
    /// xxhash64 of the raw file content (for change detection).
    pub content_hash: u64,
    /// All definitions found in this file.
    pub definitions: Vec<Definition>,
    /// All references found in this file.
    pub references: Vec<Reference>,
    /// All imports found in this file.
    pub imports: Vec<Import>,
    /// External endpoints (HTTP routes, gRPC services, etc.).
    pub external_endpoints: Vec<ExternalEndpoint>,
    /// Wall-clock microseconds spent parsing this file.
    pub parse_duration_us: u64,
}

impl FileIndex {
    /// Assemble an index from a parse of `content` attributed to `file_path`.
    ///
    /// The content hash is computed here so every caller hashes the same bytes
    /// the same way, and `parse_duration_us` starts at 0 for callers that do
    /// not time the parse (set the field afterwards if you do).
    pub fn from_parse(file_path: &str, content: &str, parsed: ParseResult) -> Self {
        Self {
            file_path: file_path.to_string(),
            content_hash: xxhash_rust::xxh64::xxh64(content.as_bytes(), 0),
            definitions: parsed.definitions,
            references: parsed.references,
            imports: parsed.imports,
            external_endpoints: parsed.external_endpoints,
            parse_duration_us: 0,
        }
    }
}

/// Thread-safe per-file parse cache shared by every language resolver.
///
/// Each `LanguageResolver` used to hand-roll an identical
/// `Mutex<HashMap<PathBuf, ParseResult>>` field plus the same
/// `get_cached`/`resolve_definitions`/`resolve_references` bodies. This newtype
/// is the single source of truth for that storage and its lookups, so the
/// per-language resolvers reduce to one-line delegations.
#[derive(Default)]
pub struct ParseCache {
    inner: Mutex<HashMap<PathBuf, ParseResult>>,
}

impl ParseCache {
    /// Insert (or replace) the cached parse result for `path`.
    pub fn insert(&self, path: &Path, result: ParseResult) {
        self.inner
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), result);
    }

    /// Return a clone of the cached parse result for `path`, if present.
    pub fn get(&self, path: &Path) -> Option<ParseResult> {
        self.inner.lock().unwrap().get(path).cloned()
    }

    /// Cached definitions for `path`, or empty when the file was never parsed.
    pub fn definitions_for(&self, path: &Path) -> Vec<Definition> {
        self.inner
            .lock()
            .unwrap()
            .get(path)
            .map(|r| r.definitions.clone())
            .unwrap_or_default()
    }

    /// Cached references for `path`, or empty when the file was never parsed.
    pub fn references_for(&self, path: &Path) -> Vec<Reference> {
        self.inner
            .lock()
            .unwrap()
            .get(path)
            .map(|r| r.references.clone())
            .unwrap_or_default()
    }

    /// Lock the underlying map for the multi-step lookups `resolve_call_edge`
    /// performs (a `get` plus iteration held under one guard).
    pub fn lock(&self) -> MutexGuard<'_, HashMap<PathBuf, ParseResult>> {
        self.inner.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cache_insert_get_roundtrip() {
        let cache = ParseCache::default();
        let path = Path::new("src/foo.rs");
        assert!(cache.get(path).is_none());

        let result = ParseResult {
            definitions: vec![Definition {
                complexity: 1,
                is_trivial_wrapper_body: false,
                name: "foo".to_string(),
                kind: NodeKind::Function,
                signature: "fn foo()".to_string(),
                file_path: "src/foo.rs".to_string(),
                line_start: 1,
                line_end: 3,
                docstring: None,
                is_public: true,
                type_hints_present: true,
                body_text: String::new(),
                in_test_context: false,
                in_trait_context: false,
                is_associated: false,
                is_auto_invoked: false,
                is_decorated: false,
                has_keep_marker: false,
                is_macro: false,
            }],
            references: vec![],
            imports: vec![],
            external_endpoints: vec![],
        };
        cache.insert(path, result);

        let got = cache.get(path).expect("value should be cached");
        assert_eq!(got.definitions.len(), 1);
        assert_eq!(cache.definitions_for(path).len(), 1);
        assert_eq!(cache.definitions_for(path)[0].name, "foo");
        assert!(cache.references_for(path).is_empty());
    }
}
