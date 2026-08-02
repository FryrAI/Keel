//! Tier-1 parsing of source held in memory rather than on disk.
//!
//! `keel review` has to parse the **base** side of a diff, which exists only as
//! a git blob (`git show <base>:<path>`). The enabling primitive is already in
//! the frozen `LanguageResolver` contract — `parse_file(&self, path, content)`
//! takes the content as a string — so no checkout, worktree, or stored history
//! is needed.
//!
//! Deliberately **Tier 1 only**: the resolvers here are built with their plain
//! constructors (`PyResolver::new`, not `detect`; `TsResolver::new`, not
//! `with_project_root`), so no `ty` subprocess is probed and no tsconfig tree is
//! walked. The output is used for *definitions* — signature, flags, body hash —
//! and never for cross-file edge resolution, where an asymmetric tier between
//! the two sides would manufacture phantom differences.

use std::path::Path;

use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::{detect_language, is_typescript_family};
use keel_parsers::typescript::TsResolver;

/// The resolution tier every index produced by this module carries.
///
/// Reported verbatim by `keel review` so a consumer can see that the base-side
/// facts came from tree-sitter alone.
pub const BLOB_RESOLUTION_TIER: &str = "tier1";

/// Lazily-built set of Tier-1 resolvers, one per language family.
///
/// Building a resolver allocates a tree-sitter parser, so a review of a
/// single-language diff must not pay for the other three.
#[derive(Default)]
pub struct BlobParser {
    ts: Option<TsResolver>,
    py: Option<PyResolver>,
    go: Option<GoResolver>,
    rust: Option<RustLangResolver>,
}

impl BlobParser {
    /// Create a parser with no resolvers built yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse in-memory `content` attributed to `path` into a [`FileIndex`].
    ///
    /// `path` is used both for language detection and as the index's
    /// `file_path`, so a base-side blob keeps its *base-side* path — which is
    /// what makes hash comparison across a rename well-defined.
    /// Returns `None` for a path in no language keel parses.
    pub fn parse(&mut self, path: &str, content: &str) -> Option<FileIndex> {
        let lang = detect_language(Path::new(path))?;
        let resolver: &dyn LanguageResolver = if is_typescript_family(lang) {
            self.ts.get_or_insert_with(TsResolver::new)
        } else {
            match lang {
                "python" => self.py.get_or_insert_with(PyResolver::new),
                "go" => self.go.get_or_insert_with(GoResolver::new),
                "rust" => self.rust.get_or_insert_with(RustLangResolver::new),
                _ => return None,
            }
        };

        let parsed = resolver.parse_file(Path::new(path), content);
        Some(FileIndex {
            file_path: path.to_string(),
            content_hash: xxhash_rust::xxh64::xxh64(content.as_bytes(), 0),
            definitions: parsed.definitions,
            references: parsed.references,
            imports: parsed.imports,
            external_endpoints: parsed.external_endpoints,
            parse_duration_us: 0,
        })
    }
}

/// Parse `(path, content)` pairs into `FileIndex` entries, skipping paths in
/// languages keel has no grammar for.
///
/// Sharing one [`BlobParser`] across the batch is the point: each language's
/// resolver is constructed at most once for the whole diff.
pub fn parse_blobs_to_indices(blobs: &[(String, String)]) -> Vec<FileIndex> {
    let mut parser = BlobParser::new();
    blobs
        .iter()
        .filter_map(|(path, content)| parser.parse(path, content))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_language_family_from_memory() {
        let blobs = vec![
            (
                "src/lib.rs".to_string(),
                "pub fn a(x: u8) -> u8 { x }\n".to_string(),
            ),
            (
                "src/app.ts".to_string(),
                "export function b(x: number): number { return x; }\n".to_string(),
            ),
            (
                "app/m.py".to_string(),
                "def c(x: int) -> int:\n    return x\n".to_string(),
            ),
            (
                "cmd/m.go".to_string(),
                "package main\n\nfunc d(x int) int {\n\treturn x\n}\n".to_string(),
            ),
        ];
        let indices = parse_blobs_to_indices(&blobs);
        assert_eq!(indices.len(), 4);
        for idx in &indices {
            assert!(
                !idx.definitions.is_empty(),
                "{} produced no definitions",
                idx.file_path
            );
        }
    }

    #[test]
    fn skips_paths_with_no_grammar() {
        let blobs = vec![
            ("migrations/001.sql".to_string(), "SELECT 1;".to_string()),
            ("README.md".to_string(), "# hi".to_string()),
        ];
        assert!(parse_blobs_to_indices(&blobs).is_empty());
    }

    #[test]
    fn keeps_the_supplied_path_not_the_disk_path() {
        let mut parser = BlobParser::new();
        let idx = parser
            .parse("old/name.rs", "pub fn a() -> u8 { 1 }\n")
            .expect("rust blob should parse");
        assert_eq!(idx.file_path, "old/name.rs");
        assert_eq!(idx.definitions[0].file_path, "old/name.rs");
    }
}
