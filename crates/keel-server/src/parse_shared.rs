//! Shared file-parsing helpers used by both MCP and HTTP handlers.

use std::path::{Path, PathBuf};

use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::typescript::TsResolver;

/// Map a path to the resolver family that parses it.
///
/// Delegates to the canonical extension table in
/// [`keel_parsers::treesitter::detect_language`] so the server never drifts
/// from what the parsers actually support, then collapses the TS-family
/// grammars (`tsx`, `javascript`, `svelte`) onto the TypeScript resolver.
pub(crate) fn detect_language(path: &str) -> Option<&'static str> {
    let lang = keel_parsers::treesitter::detect_language(Path::new(path))?;
    if keel_parsers::treesitter::is_typescript_family(lang) {
        Some("typescript")
    } else {
        Some(lang)
    }
}

/// Per-request file parser that builds each language resolver on first use.
///
/// Constructing the TypeScript resolver performs tsconfig discovery (path
/// aliases, `extends` chains), so it must happen at most once per request —
/// not per file, and not at all for requests with no TS-family files — and
/// with the project root, matching the CLI's resolution behavior.
pub(crate) struct FileParser {
    root: PathBuf,
    ts: Option<TsResolver>,
    py: Option<keel_parsers::python::PyResolver>,
    go: Option<keel_parsers::go::GoResolver>,
    rust: Option<keel_parsers::rust_lang::RustLangResolver>,
}

impl FileParser {
    /// Root the parser at the current working directory — the project root
    /// for both `keel serve` modes. Cheap: resolvers build lazily.
    pub(crate) fn new() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ts: None,
            py: None,
            go: None,
            rust: None,
        }
    }

    /// Parse a single file from disk into a `FileIndex`.
    pub(crate) fn parse(&mut self, path: &str) -> Option<FileIndex> {
        let content = std::fs::read_to_string(path).ok()?;
        let resolver: &dyn LanguageResolver = match detect_language(path)? {
            "typescript" => self
                .ts
                .get_or_insert_with(|| TsResolver::with_project_root(&self.root)),
            "python" => self
                .py
                .get_or_insert_with(keel_parsers::python::PyResolver::detect),
            "go" => self
                .go
                .get_or_insert_with(keel_parsers::go::GoResolver::new),
            "rust" => self
                .rust
                .get_or_insert_with(keel_parsers::rust_lang::RustLangResolver::new),
            _ => return None,
        };

        let parsed = resolver.parse_file(Path::new(path), &content);
        let content_hash = xxhash_rust::xxh64::xxh64(content.as_bytes(), 0);

        Some(FileIndex {
            file_path: path.to_string(),
            content_hash,
            definitions: parsed.definitions,
            references: parsed.references,
            imports: parsed.imports,
            external_endpoints: parsed.external_endpoints,
            parse_duration_us: 0,
        })
    }
}
