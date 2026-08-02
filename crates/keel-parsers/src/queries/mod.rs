use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

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

/// The tree-sitter query for the given language name, compiled at most once
/// per process.
///
/// `Query::new` re-parses and re-compiles the whole `.scm` source every time it
/// is called — tens of milliseconds, which on the `keel compile` hot path
/// dominated the actual parse. The compiled query is immutable and `Sync`, so
/// it is compiled on first use and leaked: there are at most a handful of
/// languages, and the alternative (an `Arc` handed to every caller) buys
/// nothing a process-lifetime borrow does not already give.
pub fn query_for_language(lang: &Language, lang_name: &str) -> Result<&'static Query, String> {
    static COMPILED: OnceLock<Mutex<HashMap<String, &'static Query>>> = OnceLock::new();
    // Concatenated once per process; every other arm stays a zero-copy static.
    static TSX_QUERIES: OnceLock<String> = OnceLock::new();

    let cache = COMPILED.get_or_init(|| Mutex::new(HashMap::new()));
    // A poisoned lock means some other thread panicked mid-compile; the map is
    // still a valid cache, so keep using it rather than failing every parse.
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(query) = cache.get(lang_name) {
        return Ok(query);
    }

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
    let query = Query::new(lang, source)
        .map_err(|e| format!("query compilation error for {lang_name}: {e}"))?;
    let query: &'static Query = Box::leak(Box::new(query));
    cache.insert(lang_name.to_string(), query);
    Ok(query)
}
