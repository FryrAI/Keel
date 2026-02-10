//! Shared file-parsing logic used by both `compile` and `serve --watch`.

use std::fs;
use std::path::Path;

use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::detect_language;
use keel_parsers::typescript::TsResolver;

/// Parse a list of file paths into `FileIndex` entries suitable for `engine.compile()`.
///
/// Skips files with unrecognized extensions or read errors.
pub fn parse_files_to_indices(
    file_paths: &[std::path::PathBuf],
    root_dir: &Path,
) -> Vec<FileIndex> {
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go_resolver = GoResolver::new();
    let rs = RustLangResolver::new();

    let mut indices = Vec::new();

    for file_path in file_paths {
        let lang = match detect_language(file_path) {
            Some(l) => l,
            None => continue,
        };

        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let resolver: &dyn LanguageResolver = match lang {
            "typescript" | "javascript" | "tsx" => &ts,
            "python" => &py,
            "go" => &go_resolver,
            "rust" => &rs,
            _ => continue,
        };

        let result = resolver.parse_file(file_path, &content);
        let rel_path = make_relative(root_dir, file_path);

        let content_hash = {
            let mut h: u64 = 0;
            for byte in content.as_bytes() {
                h = h.wrapping_mul(31).wrapping_add(*byte as u64);
            }
            h
        };

        indices.push(FileIndex {
            file_path: rel_path,
            content_hash,
            definitions: result.definitions,
            references: result.references,
            imports: result.imports,
            external_endpoints: result.external_endpoints,
            parse_duration_us: 0,
        });
    }

    indices
}

fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
