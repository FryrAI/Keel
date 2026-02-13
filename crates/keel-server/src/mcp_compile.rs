//! MCP compile handler — parses files and runs enforcement.
//!
//! Uses a shared EnforcementEngine so circuit breaker, batch mode,
//! and graph state persist across MCP calls within a session.

use std::path::Path;

use serde_json::Value;

use keel_enforce::types::{CompileInfo, CompileResult};
use keel_parsers::resolver::{FileIndex, LanguageResolver};

use crate::mcp::{internal_err, JsonRpcError, SharedEngine};

pub(crate) fn handle_compile(engine: &SharedEngine, params: Option<Value>) -> Result<Value, JsonRpcError> {
    let files: Vec<String> = params
        .as_ref()
        .and_then(|p| p.get("files").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let batch_start = params
        .as_ref()
        .and_then(|p| p.get("batch_start"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let batch_end = params
        .as_ref()
        .and_then(|p| p.get("batch_end"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Parse files that exist on disk into FileIndexes
    let file_indexes: Vec<FileIndex> = files
        .iter()
        .filter_map(|path| parse_file_to_index(path))
        .collect();

    // Use shared engine — state persists across calls
    let mut engine = engine.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Engine lock poisoned".into(),
    })?;

    // Run actual enforcement if we have parseable files
    if !file_indexes.is_empty() || batch_start || batch_end {
        if batch_start {
            engine.batch_start();
        }

        let mut result = engine.compile(&file_indexes);
        // Include all requested files in files_analyzed, not just parseable ones
        result.files_analyzed = files;

        // Override status for batch mode signals
        if batch_start {
            result.status = "batch_started".to_string();
        }

        if batch_end {
            let batch_result = engine.batch_end();
            result.errors.extend(batch_result.errors);
            result.warnings.extend(batch_result.warnings);
            result.status = "batch_ended".to_string();
        }

        return serde_json::to_value(result).map_err(internal_err);
    }

    // Fallback for empty/no-file requests
    let result = CompileResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "compile".into(),
        status: "ok".into(),
        files_analyzed: files,
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };

    serde_json::to_value(result).map_err(internal_err)
}

/// Detect language from file extension.
fn detect_language(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()? {
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => Some("typescript"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        _ => None,
    }
}

/// Parse a single file from disk into a FileIndex.
fn parse_file_to_index(path: &str) -> Option<FileIndex> {
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
