use std::fs;
use std::path::Path;
use std::time::Instant;

use keel_output::OutputFormatter;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::detect_language;
use keel_parsers::typescript::TsResolver;

use super::compile_lock::acquire_compile_lock;
use super::compile_metrics::build_compile_metrics;
use crate::telemetry_recorder::EventMetrics;

/// Supported file extensions for --changed filtering.
const SUPPORTED_EXTENSIONS: &[&str] = &["rs", "py", "ts", "tsx", "js", "jsx", "go"];

/// Run `keel compile` — incremental validation of changed files.
#[allow(clippy::too_many_arguments)]
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    files: Vec<String>,
    batch_start: bool,
    batch_end: bool,
    strict: bool,
    suppress: Option<String>,
    _depth: u32,
    changed: bool,
    since: Option<String>,
    delta: bool,
    timeout: Option<u64>,
) -> (i32, EventMetrics) {
    let start = Instant::now();

    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel compile: failed to get current directory: {}", e);
            return (2, EventMetrics::default());
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel compile: not initialized. Run `keel init` first.");
        return (2, EventMetrics::default());
    }

    // Acquire compile lock to prevent concurrent corruption
    let _lock = match acquire_compile_lock(&keel_dir, verbose) {
        Some(lock) => lock,
        None => {
            eprintln!("keel compile: another compile is running, skipping");
            return (0, EventMetrics::default());
        }
    };

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel compile: failed to open graph database: {}", e);
            return (2, EventMetrics::default());
        }
    };

    // Load persisted circuit breaker state
    let cb_state = store.load_circuit_breaker().unwrap_or_default();

    let config = keel_core::config::KeelConfig::load(&keel_dir);
    let mut engine = keel_enforce::engine::EnforcementEngine::with_config(Box::new(store), &config);
    engine.import_circuit_breaker(&cb_state);

    // Apply suppressions
    if let Some(code) = &suppress {
        engine.suppress(code);
    }

    // Handle batch mode
    if batch_start {
        engine.batch_start();
        if verbose {
            eprintln!("keel compile: batch mode started");
        }
        return (0, EventMetrics::default());
    }

    if batch_end {
        let result = engine.batch_end();
        let exit = output_result(formatter, &result, strict, verbose);
        let metrics = build_compile_metrics(&result, &[]);
        return (exit, metrics);
    }

    // Resolve target files: --changed, --since, explicit list, or all
    let mut effective_files = files;
    if changed || since.is_some() {
        match git_changed_files(&since) {
            Ok(git_files) => {
                if verbose {
                    eprintln!("keel compile: {} file(s) changed in git", git_files.len());
                }
                effective_files = git_files;
            }
            Err(e) => {
                eprintln!("keel compile: git diff failed: {}", e);
                return (2, EventMetrics::default());
            }
        }
    }

    // Parse target files into FileIndex entries.
    let mut ts: Option<TsResolver> = None;
    let mut py: Option<PyResolver> = None;
    let mut go_resolver: Option<GoResolver> = None;
    let mut rs: Option<RustLangResolver> = None;

    let target_files = if effective_files.is_empty() {
        let walker = keel_parsers::walker::FileWalker::new(&cwd);
        walker
            .walk()
            .into_iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
    } else {
        effective_files
            .iter()
            .map(|f| {
                let p = Path::new(f);
                if p.is_absolute() {
                    f.clone()
                } else {
                    cwd.join(f).to_string_lossy().to_string()
                }
            })
            .collect::<Vec<_>>()
    };

    let mut file_indices: Vec<FileIndex> = Vec::new();

    for file_str in &target_files {
        let file_path = Path::new(file_str);
        let lang = match detect_language(file_path) {
            Some(l) => l,
            None => continue,
        };
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                if verbose {
                    eprintln!("keel compile: skipping {}: {}", file_str, e);
                }
                continue;
            }
        };

        let resolver: &dyn LanguageResolver = match lang {
            "typescript" | "javascript" | "tsx" => ts.get_or_insert_with(TsResolver::new),
            "python" => py.get_or_insert_with(PyResolver::new),
            "go" => go_resolver.get_or_insert_with(GoResolver::new),
            "rust" => rs.get_or_insert_with(RustLangResolver::new),
            _ => continue,
        };

        let result = resolver.parse_file(file_path, &content);
        let rel_path = make_relative(&cwd, file_path);
        let content_hash = xxhash_rust::xxh64::xxh64(content.as_bytes(), 0);

        file_indices.push(FileIndex {
            file_path: rel_path,
            content_hash,
            definitions: result.definitions,
            references: result.references,
            imports: result.imports,
            external_endpoints: result.external_endpoints,
            parse_duration_us: 0,
        });
    }

    if verbose && !file_indices.is_empty() {
        eprintln!("keel compile: checking {} file(s)", file_indices.len());
    }

    let result = engine.compile(&file_indices);

    // Persist circuit breaker state back to SQLite
    let cb_out = engine.export_circuit_breaker();
    let cb_events = cb_out.len() as u32;
    if !cb_out.is_empty() {
        if let Ok(cb_store) =
            keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or(""))
        {
            if let Err(e) = cb_store.save_circuit_breaker(&cb_out) {
                if verbose {
                    eprintln!("keel compile: failed to persist circuit breaker: {}", e);
                }
            }
        }
    }

    // Build metrics before delta processing may consume result
    let mut metrics = build_compile_metrics(&result, &target_files);
    metrics.circuit_breaker_events = cb_events;

    // Delta mode: diff against previous snapshot
    if delta {
        use keel_enforce::snapshot::{compute_delta, ViolationSnapshot};

        let previous = ViolationSnapshot::load(&keel_dir);

        // Always save current snapshot
        let current_snapshot = ViolationSnapshot::from_compile_result(&result);
        if let Err(e) = current_snapshot.save(&keel_dir) {
            if verbose {
                eprintln!("keel compile: failed to save snapshot: {}", e);
            }
        }

        if let Some(prev) = previous {
            let delta_result = compute_delta(&prev, &result);
            let output = formatter.format_compile_delta(&delta_result);
            if !output.is_empty() {
                println!("{}", output);
            }
            let has_errors = !result.errors.is_empty();
            let has_warnings = !result.warnings.is_empty();
            let exit = if has_errors || (strict && has_warnings) {
                1
            } else {
                0
            };
            return (exit, metrics);
        }
        // No previous snapshot: fall through to normal output
        if verbose {
            eprintln!("keel compile: no previous snapshot, showing full results");
        }
    } else {
        // Always save snapshot even without --delta for future use
        use keel_enforce::snapshot::ViolationSnapshot;
        let snapshot = ViolationSnapshot::from_compile_result(&result);
        if let Err(e) = snapshot.save(&keel_dir) {
            if verbose {
                eprintln!("keel compile: failed to save snapshot: {}", e);
            }
        }
    }

    // Check timeout before outputting results
    if let Some(timeout_ms) = timeout {
        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed > timeout_ms {
            if verbose {
                eprintln!(
                    "keel compile: timed out ({}ms > {}ms limit)",
                    elapsed, timeout_ms
                );
            }
            return (0, metrics); // Don't block the agent
        }
    }

    let exit = output_result(formatter, &result, strict, verbose);
    (exit, metrics)
}

/// Get files changed according to git diff.
fn git_changed_files(since: &Option<String>) -> Result<Vec<String>, String> {
    let range = since.as_ref().map(|c| format!("{}..HEAD", c));
    let args: Vec<&str> = match &range {
        Some(r) => vec!["diff", "--name-only", r.as_str()],
        None => vec!["diff", "--name-only", "HEAD"],
    };

    let output = std::process::Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("failed to run git: {}", e))?;

    if !output.status.success() {
        // Fallback for initial commits (no HEAD yet)
        let fallback = std::process::Command::new("git")
            .args(["diff", "--name-only", "--cached"])
            .output()
            .map_err(|e| format!("git fallback failed: {}", e))?;
        let text = String::from_utf8_lossy(&fallback.stdout);
        return Ok(filter_supported_files(&text));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(filter_supported_files(&text))
}

/// Filter file paths to only supported extensions.
fn filter_supported_files(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.is_empty())
        .filter(|line| {
            Path::new(line)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| SUPPORTED_EXTENSIONS.contains(&e))
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .collect()
}

fn output_result(
    formatter: &dyn OutputFormatter,
    result: &keel_enforce::types::CompileResult,
    strict: bool,
    verbose: bool,
) -> i32 {
    let has_errors = !result.errors.is_empty();
    let has_warnings = !result.warnings.is_empty();

    if !has_errors && !has_warnings {
        if verbose {
            eprintln!("keel compile: clean — no violations");
        }
        return 0;
    }

    let output = formatter.format_compile(result);
    if !output.is_empty() {
        println!("{}", output);
    }

    if has_errors || (strict && has_warnings) {
        1
    } else {
        0
    }
}

/// Make a path relative to the project root.
fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
