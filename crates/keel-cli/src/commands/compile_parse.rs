//! Per-language resolver construction and single-file parsing, shared by
//! `keel compile` and the graph re-sync so both parse a file the same way.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use keel_core::paths::make_relative;
use keel_parsers::boundary::BoundaryLiterals;
use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::{detect_language, SupplementalResolver};
use keel_parsers::typescript::TsResolver;
use keel_parsers::{go::GoResolver, python::PyResolver};

use super::map_lang_resolve::ResolverSet;

/// The five lazily built resolvers of one invocation.
///
/// Each resolver is built on first use for its language, with the boundary key
/// set installed before it parses anything (empty for a repo with no boundary
/// surface, which disables literals entirely).
pub(crate) struct CompileParser {
    root: PathBuf,
    literal_keys: Arc<HashSet<String>>,
    ts: Option<TsResolver>,
    py: Option<PyResolver>,
    go: Option<GoResolver>,
    rs: Option<RustLangResolver>,
    supplemental: Option<SupplementalResolver>,
}

impl CompileParser {
    pub(crate) fn new(root: &Path, literal_keys: Arc<HashSet<String>>) -> Self {
        Self {
            root: root.to_path_buf(),
            literal_keys,
            ts: None,
            py: None,
            go: None,
            rs: None,
            supplemental: None,
        }
    }

    /// Parse `content` as the file at `abs_path`, or `None` for a path
    /// `detect_language` does not know or a language without a resolver arm.
    pub(crate) fn parse(&mut self, abs_path: &Path, content: &str) -> Option<FileIndex> {
        let lang = detect_language(abs_path)?;
        let (root, keys) = (&self.root, &self.literal_keys);
        let resolver: &dyn LanguageResolver = match lang {
            l if keel_parsers::treesitter::is_typescript_family(l) => {
                self.ts.get_or_insert_with(|| {
                    TsResolver::with_project_root(root).with_boundary_literals(keys.clone())
                })
            }
            "python" => self
                .py
                .get_or_insert_with(|| PyResolver::detect().with_boundary_literals(keys.clone())),
            "go" => self.go.get_or_insert_with(GoResolver::new),
            "rust" => self.rs.get_or_insert_with(|| {
                RustLangResolver::new().with_boundary_literals(keys.clone())
            }),
            "typst" | "astro" | "bash" | "sql" => self.supplemental.get_or_insert_with(|| {
                SupplementalResolver::with_project_root(root).with_boundary_literals(keys.clone())
            }),
            _ => return None,
        };
        let result = resolver.parse_file(abs_path, content);
        let rel_path = make_relative(&self.root, abs_path);
        Some(FileIndex::from_parse(&rel_path, content, result))
    }

    /// The same `ResolverSet` the map uses; only the languages touched so far
    /// are `Some`.
    pub(crate) fn resolver_set(&self) -> ResolverSet<'_> {
        ResolverSet {
            ts: self.ts.as_ref().map(|r| r as &dyn LanguageResolver),
            py: self.py.as_ref().map(|r| r as &dyn LanguageResolver),
            go: self.go.as_ref().map(|r| r as &dyn LanguageResolver),
            rs: self.rs.as_ref().map(|r| r as &dyn LanguageResolver),
        }
    }
}
