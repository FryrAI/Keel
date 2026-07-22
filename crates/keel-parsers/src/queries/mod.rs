use std::sync::OnceLock;

use tree_sitter::{Language, Query};

pub const TYPESCRIPT_QUERIES: &str = include_str!("typescript.scm");
pub const PYTHON_QUERIES: &str = include_str!("python.scm");
pub const GO_QUERIES: &str = include_str!("go.scm");
pub const RUST_QUERIES: &str = include_str!("rust.scm");
// TSX-grammar-only: `jsx_*` node kinds do not exist in the plain TypeScript
// grammar, so this must never be concatenated into TYPESCRIPT_QUERIES (that
// source is compiled against BOTH grammars below) — it would fail to compile
// for every plain .ts file.
pub const TYPESCRIPT_JSX_QUERIES: &str = include_str!("typescript_jsx.scm");

/// Compiles the tree-sitter query for the given language name.
pub fn query_for_language(lang: &Language, lang_name: &str) -> Result<Query, String> {
    // Concatenated once per process; every other arm stays a zero-copy static.
    static TSX_QUERIES: OnceLock<String> = OnceLock::new();
    let source: &str = match lang_name {
        "tsx" => {
            TSX_QUERIES.get_or_init(|| format!("{TYPESCRIPT_QUERIES}\n{TYPESCRIPT_JSX_QUERIES}"))
        }
        l if crate::treesitter::is_typescript_family(l) => TYPESCRIPT_QUERIES,
        "python" => PYTHON_QUERIES,
        "go" => GO_QUERIES,
        "rust" => RUST_QUERIES,
        other => return Err(format!("unsupported language: {other}")),
    };
    Query::new(lang, source).map_err(|e| format!("query compilation error for {lang_name}: {e}"))
}
