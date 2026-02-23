use tree_sitter::{Language, Query};

pub const TYPESCRIPT_QUERIES: &str = include_str!("typescript.scm");
pub const PYTHON_QUERIES: &str = include_str!("python.scm");
pub const GO_QUERIES: &str = include_str!("go.scm");
pub const RUST_QUERIES: &str = include_str!("rust.scm");

/// Compiles the tree-sitter query for the given language name.
pub fn query_for_language(lang: &Language, lang_name: &str) -> Result<Query, String> {
    let source = match lang_name {
        "typescript" | "tsx" | "javascript" => TYPESCRIPT_QUERIES,
        "python" => PYTHON_QUERIES,
        "go" => GO_QUERIES,
        "rust" => RUST_QUERIES,
        other => return Err(format!("unsupported language: {other}")),
    };
    Query::new(lang, source).map_err(|e| format!("query compilation error for {lang_name}: {e}"))
}
