use std::fs;
use std::path::Path;

use keel_output::OutputFormatter;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::detect_language;
use keel_parsers::typescript::TsResolver;

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
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel compile: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel compile: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel compile: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Load persisted circuit breaker state
    let cb_state = store.load_circuit_breaker().unwrap_or_default();

    let mut engine = keel_enforce::engine::EnforcementEngine::new(Box::new(store));
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
        return 0;
    }

    if batch_end {
        let result = engine.batch_end();
        return output_result(formatter, &result, strict, verbose);
    }

    // Parse target files into FileIndex entries.
    // Lazily create resolvers — only allocate the ones we actually need.
    let mut ts: Option<TsResolver> = None;
    let mut py: Option<PyResolver> = None;
    let mut go_resolver: Option<GoResolver> = None;
    let mut rs: Option<RustLangResolver> = None;

    let target_files = if files.is_empty() {
        // No specific files: walk all source files
        let walker = keel_parsers::walker::FileWalker::new(&cwd);
        walker
            .walk()
            .into_iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
    } else {
        // Resolve relative paths to absolute
        files
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
        // Use a simple hash of the content for change detection
        let content_hash = {
            let mut h: u64 = 0;
            for byte in content.as_bytes() {
                h = h.wrapping_mul(31).wrapping_add(*byte as u64);
            }
            h
        };

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

    output_result(formatter, &result, strict, verbose)
}

fn output_result(
    formatter: &dyn OutputFormatter,
    result: &keel_enforce::types::CompileResult,
    strict: bool,
    verbose: bool,
) -> i32 {
    // Clean compile = empty stdout, exit 0
    let has_errors = !result.errors.is_empty();
    let has_warnings = !result.warnings.is_empty();

    if !has_errors && !has_warnings {
        if verbose {
            eprintln!("keel compile: clean — no violations");
        }
        return 0;
    }

    // Output violations
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
