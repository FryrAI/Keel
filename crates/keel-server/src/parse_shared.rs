//! Shared file-parsing helpers used by both MCP and HTTP handlers.

use std::path::Path;

use keel_parsers::resolver::{FileIndex, LanguageResolver};

/// Detect language from file extension.
pub(crate) fn detect_language(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()? {
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => Some("typescript"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        _ => None,
    }
}

/// Parse a single file from disk into a FileIndex.
pub(crate) fn parse_file_to_index(path: &str) -> Option<FileIndex> {
    let content = std::fs::read_to_string(path).ok()?;
    let lang = detect_language(path)?;

    let resolver: Box<dyn LanguageResolver> = match lang {
        "typescript" => Box::new(keel_parsers::typescript::TsResolver::new()),
        "python" => Box::new(keel_parsers::python::PyResolver::new()),
        "go" => Box::new(keel_parsers::go::GoResolver::new()),
        "rust" => Box::new(keel_parsers::rust_lang::RustLangResolver::new()),
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
