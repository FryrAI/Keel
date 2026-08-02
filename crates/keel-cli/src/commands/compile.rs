use std::fs;
use std::path::Path;
use std::time::Instant;

use keel_enforce::batch::PersistentBatch;
use keel_output::OutputFormatter;
use keel_parsers::boundary::BoundaryLiterals;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{FileIndex, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::detect_language;
use keel_parsers::typescript::TsResolver;

use super::compile_lock::acquire_compile_lock;
use super::compile_metrics::build_compile_metrics;
use crate::telemetry_recorder::EventMetrics;
use keel_core::paths::make_relative;

/// Everything `keel compile` was asked to do, straight off the command line.
///
/// A struct rather than a thirteenth positional parameter: every flag here is
/// a `bool` or an `Option<String>`, and at that arity the compiler stops being
/// able to tell them apart.
pub struct CompileArgs {
    /// Files to compile; empty means "whatever git says changed".
    pub files: Vec<String>,
    pub batch_start: bool,
    pub batch_end: bool,
    /// Treat warnings as errors.
    pub strict: bool,
    /// A single code to downgrade to S001/INFO for this run.
    pub suppress: Option<String>,
    /// Scope to the working-tree diff against HEAD.
    pub changed: bool,
    /// Scope to the committed range `<commit>..HEAD`.
    pub since: Option<String>,
    /// Report only what changed since the last compile's snapshot.
    pub delta: bool,
    /// Soft time budget in milliseconds.
    pub timeout: Option<u64>,
    /// Emit GitHub Actions workflow commands instead of a keel format.
    pub github: bool,
}

/// Run `keel compile` — incremental validation of changed files.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    args: CompileArgs,
) -> (i32, EventMetrics) {
    let CompileArgs {
        files,
        batch_start,
        batch_end,
        strict,
        suppress,
        changed,
        since,
        delta,
        timeout,
        github,
    } = args;
    let start = Instant::now();

    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel compile: failed to get current directory: {}", e);
            return (2, EventMetrics::default());
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&cwd);
    if !keel_dir.exists() {
        eprintln!("keel compile: not initialized. Run `keel init` first.");
        return (2, EventMetrics::default());
    }

    // One config load serves both the drift check and the engine below.
    let config = keel_core::config::KeelConfig::load(&keel_dir);

    // Detect (never rewrite — Principle 7) a binary/docs version mismatch.
    // At most one line, emitted once per invocation.
    super::version_drift::warn(&cwd, &config);

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

    // Refuse to enforce against a graph built on a commit this checkout does
    // not contain: every caller and every removal it reports would be phantom.
    // Exit 2 (internal error), never 0 — a stale graph must fail loudly, which
    // is the one thing "no graph" already does for free.
    if let Some(msg) = super::graph_staleness::stale_graph_message(&cwd, &store) {
        eprintln!("{msg}");
        return (2, EventMetrics::default());
    }

    // Handle batch mode. State is persisted in SQLite (a `keel_meta` row next
    // to the circuit-breaker state) so it survives across separate `keel
    // compile` processes — otherwise `--batch-start` sets state in a per-process
    // engine that is dropped at exit, silently losing every deferred violation.
    // These run on `store` before it is handed to the engine.
    if batch_start {
        if let Err(e) = PersistentBatch::new().save(&store) {
            eprintln!("keel compile: failed to start batch: {}", e);
            return (2, EventMetrics::default());
        }
        if verbose {
            eprintln!("keel compile: batch mode started");
        }
        return (0, EventMetrics::default());
    }

    if batch_end {
        let deferred = PersistentBatch::load(&store)
            .map(|b| b.drain())
            .unwrap_or_default();
        PersistentBatch::clear(&store);
        let result = keel_enforce::engine::EnforcementEngine::result_from_violations(deferred);
        let exit = output_result(formatter, &result, strict, verbose, github);
        let metrics = build_compile_metrics(&result, &[]);
        return (exit, metrics);
    }

    // If a batch is active and not expired, this compile defers its deferrable
    // violations into the persisted batch rather than firing them now. Reading
    // it is a query on the already-open handle — no file I/O — and on the common
    // no-batch path it is a single lookup that returns `None`.
    let active_batch = PersistentBatch::load(&store);
    let batch_deferring = active_batch.as_ref().is_some_and(|b| !b.is_expired());

    // Load persisted circuit breaker state
    let cb_state = store.load_circuit_breaker().unwrap_or_default();

    // The boundary-name key set for string-literal dispatch references, read
    // from the graph the last `keel map` wrote (this pipeline never re-scans
    // the boundary surface). It must be installed on the resolvers below:
    // compile prunes and re-resolves each touched file's outgoing edges, so a
    // literal edge it cannot reproduce would be deleted until the next map.
    let literal_keys = super::map_boundary::persisted_literal_keys(&cwd, &store);

    // Whether the user explicitly named the target files (not --changed/--since,
    // not a bare full-repo compile). Only then is a missing file a hard error;
    // git-deleted paths under --changed/--since must still skip silently.
    let explicit_targets = !changed && since.is_none() && !files.is_empty();

    // A bare `keel compile` — no explicit files, no --changed, no --since —
    // used to fall through to an unscoped walk of the ENTIRE repo: every file
    // re-parsed and re-checked against the stored graph with no caching
    // (~15s on a mid-size repo vs <100ms for one scoped file; a hook that
    // drops its file argument silently pays this on every edit). The CLI help
    // text already promised "empty = all changed"; in a git repository, bare
    // compile now actually defaults to the same working-tree-vs-HEAD diff as
    // `--changed`, closing that gap. Repos with no `.git` (integration-test
    // fixtures, non-git checkouts) keep the old full-repo-scan default, since
    // there is no git history to scope against.
    let bare_compile = files.is_empty() && !changed && since.is_none();
    let default_to_changed = bare_compile && cwd.join(".git").exists();

    // Resolve target files: --changed, --since, default-to-changed, explicit
    // list, or (only for a non-git bare compile) all.
    //
    // `untracked_targets` holds the paths that were *asked about* but that keel
    // has no grammar for. Only these produce the untracked-language notice
    // below: a repo-wide walk must never emit it, or every compile in a repo
    // with one `.sql` file carries a permanent warning.
    let mut effective_files = files;
    let mut untracked_targets: Vec<String> = effective_files.clone();
    if changed || since.is_some() || default_to_changed {
        match git_changed_files(&since) {
            Ok(git_files) => {
                effective_files = git_files
                    .iter()
                    .filter(|f| detect_language(Path::new(f)).is_some())
                    .cloned()
                    .collect();
                untracked_targets = git_files;
                if verbose {
                    eprintln!(
                        "keel compile: {} file(s) changed in git",
                        effective_files.len()
                    );
                }
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

    // A full-repo compile (a bare compile that could not be scoped to git
    // changes) is close to a `keel map`; the incremental graph sync below is
    // skipped for it to avoid degrading the map-built edge graph with weaker
    // single-file resolution. This must NOT fire merely because a git-scoped
    // diff (explicit or default) happened to return zero files — that means
    // "nothing to do", not "scan everything".
    let full_repo_compile = bare_compile && !default_to_changed;

    let target_files = if full_repo_compile {
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

    // An explicitly-named target that does not exist is a hard error, not a
    // silent exit 0: the agent asked us to validate a specific file and we
    // could not. (Git-deleted paths under --changed/--since are excluded above.)
    if explicit_targets {
        for path in &target_files {
            if !Path::new(path).exists() {
                eprintln!("keel compile: file not found: {}", path);
                return (2, EventMetrics::default());
            }
        }
    }

    // Exit 0 on a file keel never parsed is a false all-clear for any hook that
    // reads the exit code as verification. One line, for structurally
    // significant extensions only (never .md/.json/.lock).
    if let Some(notice) = keel_enforce::file_class::untracked_language_notice(&untracked_targets) {
        eprintln!("{}", notice);
    }

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
                if e.kind() == std::io::ErrorKind::InvalidData {
                    // Not valid UTF-8: the file was NOT validated. Always tell
                    // the agent (not just under --verbose) — an unvalidated file
                    // must never be mistaken for a clean one.
                    eprintln!(
                        "keel compile: skipped (not valid UTF-8, NOT validated): {}",
                        file_str
                    );
                } else if verbose {
                    eprintln!("keel compile: skipping {}: {}", file_str, e);
                }
                continue;
            }
        };

        // Each resolver is built on first use for its language, with the
        // boundary key set installed before it parses anything (empty for a
        // repo with no boundary surface, which disables literals entirely).
        let resolver: &dyn LanguageResolver = match lang {
            l if keel_parsers::treesitter::is_typescript_family(l) => ts.get_or_insert_with(|| {
                TsResolver::with_project_root(&cwd).with_boundary_literals(literal_keys.clone())
            }),
            "python" => py.get_or_insert_with(|| {
                PyResolver::detect().with_boundary_literals(literal_keys.clone())
            }),
            "go" => go_resolver.get_or_insert_with(GoResolver::new),
            "rust" => rs.get_or_insert_with(|| {
                RustLangResolver::new().with_boundary_literals(literal_keys.clone())
            }),
            _ => continue,
        };

        let result = resolver.parse_file(file_path, &content);
        let rel_path = make_relative(&cwd, file_path);

        file_indices.push(FileIndex::from_parse(&rel_path, &content, result));
    }

    if verbose && !file_indices.is_empty() {
        eprintln!("keel compile: checking {} file(s)", file_indices.len());
    }

    // The same `ResolverSet` the map uses; `keel compile` only constructs
    // resolvers for the languages it touched, so the untouched fields stay
    // `None`. Shared by the pre-enforcement resolution pass here and the
    // post-enforcement edge sync below.
    let resolvers = super::map_lang_resolve::ResolverSet {
        ts: ts.as_ref().map(|r| r as &dyn LanguageResolver),
        py: py.as_ref().map(|r| r as &dyn LanguageResolver),
        go: go_resolver.as_ref().map(|r| r as &dyn LanguageResolver),
        rs: rs.as_ref().map(|r| r as &dyn LanguageResolver),
    };

    // Resolve call references against the pre-edit graph so E005 arity
    // checking has real targets — before the engine takes ownership of the
    // store, and before enforcement mutates anything.
    super::compile_sync::resolve_call_targets(&store, &cwd, &mut file_indices, &resolvers);

    let mut engine = keel_enforce::engine::EnforcementEngine::with_config(Box::new(store), &config);
    engine.import_circuit_breaker(&cb_state);

    // Apply suppressions
    if let Some(code) = &suppress {
        engine.suppress(code);
    }

    if batch_deferring {
        engine.batch_start();
    }

    let result = engine.compile(&file_indices);

    // Persist circuit-breaker state and run the incremental graph sync. Both
    // run *after* enforcement, sequentially, so they share ONE post-enforcement
    // SQLite handle instead of opening (and re-initializing the schema on) a
    // separate connection each. It is a fresh handle rather than the engine's
    // own because the engine still owns its connection here and the sync needs
    // `SqliteGraphStore` inherent methods the frozen `GraphStore` trait does not
    // expose; E001/E004 already diffed against the pre-edit graph.
    let cb_out = engine.export_circuit_breaker();
    let cb_events = cb_out.len() as u32;
    let need_sync = !full_repo_compile && !file_indices.is_empty();
    // One post-enforcement handle, reused for circuit-breaker persistence, the
    // incremental sync, and the batch-state writes below. Opened only when
    // something actually needs it, so the clean no-work path adds no connection.
    let mut post_store = if !cb_out.is_empty() || need_sync || active_batch.is_some() {
        match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
            Ok(s) => Some(s),
            Err(e) => {
                if verbose {
                    eprintln!(
                        "keel compile: failed to open graph for post-compile work: {}",
                        e
                    );
                }
                None
            }
        }
    } else {
        None
    };
    if let Some(ps) = post_store.as_mut() {
        if !cb_out.is_empty() {
            if let Err(e) = ps.save_circuit_breaker(&cb_out) {
                if verbose {
                    eprintln!("keel compile: failed to persist circuit breaker: {}", e);
                }
            }
        }
        if need_sync {
            super::compile_sync::sync_compiled_files(ps, &cwd, &file_indices, &resolvers, verbose);
        }
    }

    // Build metrics before delta processing may consume result
    let mut metrics = build_compile_metrics(&result, &target_files);
    metrics.circuit_breaker_events = cb_events;

    // Batch-mode bookkeeping: persist this compile's deferred violations,
    // honoring the documented 60s inactivity auto-expire. Reuses the
    // `post_store` handle opened above (guaranteed present when a batch is
    // active, unless the reopen failed).
    if let Some(mut batch) = active_batch {
        if batch.is_expired() {
            // Expired: end the batch and fire the accumulated deferred alongside
            // this compile's own violations (which were NOT deferred, since
            // `batch_deferring` was false above).
            if let Some(ps) = post_store.as_ref() {
                PersistentBatch::clear(ps);
            }
            if verbose {
                eprintln!("keel compile: batch expired (60s inactivity); firing deferred");
            }
            let mut all = batch.clone().drain();
            all.extend(result.errors.iter().cloned());
            all.extend(result.warnings.iter().cloned());
            let merged = keel_enforce::engine::EnforcementEngine::result_from_violations(all);
            let exit = output_result(formatter, &merged, strict, verbose, github);
            return (exit, metrics);
        }
        // Still batching: stash this compile's deferred, keep the timer warm.
        batch.extend_deferred(engine.drain_batch_deferred());
        batch.touch();
        if let Some(ps) = post_store.as_ref() {
            if let Err(e) = batch.save(ps) {
                if verbose {
                    eprintln!("keel compile: failed to persist batch: {}", e);
                }
            }
        }
        // Only immediate (structural) violations fire during a batch.
        let exit = output_result(formatter, &result, strict, verbose, github);
        return (exit, metrics);
    }

    // Compute adoption metrics: resolved/persisted/new vs previous snapshot.
    // This runs on every compile, not just --delta, so telemetry always has the data.
    {
        use keel_enforce::snapshot::{compute_delta, ViolationSnapshot};
        if let Some(prev) = ViolationSnapshot::load(&keel_dir) {
            let delta = compute_delta(&prev, &result);
            metrics.violations_resolved =
                (delta.resolved_errors.len() + delta.resolved_warnings.len()) as u32;
            metrics.violations_persisted = {
                let total_prev = prev.errors.len() + prev.warnings.len();
                let resolved = delta.resolved_errors.len() + delta.resolved_warnings.len();
                total_prev.saturating_sub(resolved) as u32
            };
            metrics.violations_new = (delta.new_errors.len() + delta.new_warnings.len()) as u32;
        }
    }

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
            let output = if github {
                github_delta_annotations(&result, &delta_result)
            } else {
                formatter.format_compile_delta(&delta_result)
            };
            if !output.is_empty() {
                println!("{}", output);
            }
            let has_errors = !delta_result.new_errors.is_empty();
            let has_warnings = !delta_result.new_warnings.is_empty();
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

    // Note (but do not act on) an exceeded time budget. Exceeding the budget
    // must NOT mask violations as exit 0 — the results were already computed, so
    // we still output them and return the real exit code. A stale-but-honest
    // result is far better than a false all-clear.
    if let Some(timeout_ms) = timeout {
        let elapsed = start.elapsed().as_millis() as u64;
        if elapsed > timeout_ms {
            eprintln!(
                "keel compile: exceeded time budget ({}ms > {}ms); results may be from a slow run",
                elapsed, timeout_ms
            );
        }
    }

    let exit = output_result(formatter, &result, strict, verbose, github);
    (exit, metrics)
}

/// Get files changed according to git diff.
///
/// Unfiltered: the caller keeps the parseable ones as compile targets and reads
/// the rest to decide whether a changed `.sql`/`.baml`/`.proto`/`.graphql` file
/// deserves the untracked-language notice. Filtering here would hide those
/// paths from the very check that exists to stop them passing silently.
fn git_changed_files(since: &Option<String>) -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("failed to get cwd: {}", e))?;
    let mode = match since {
        // `--since <commit>` means the committed range `<commit>..HEAD`.
        Some(base) => keel_enforce::gitdiff::DiffMode::Range(base.clone()),
        // `--changed` means working tree vs HEAD.
        None => keel_enforce::gitdiff::DiffMode::Since(None),
    };
    keel_enforce::gitdiff::changed_files_checked(&cwd, &mode, false)
}

/// Annotations for the violations `--delta` calls new.
///
/// A delta carries `ViolationKey`s, which have no message to annotate with, so
/// the full violations are re-selected from this compile's own result by their
/// stable identity. Without this, asking for workflow commands *and* a delta
/// would silently hand CI a keel-format delta report instead.
fn github_delta_annotations(
    result: &keel_enforce::types::CompileResult,
    delta: &keel_enforce::types::CompileDelta,
) -> String {
    let new_keys: std::collections::HashSet<_> = delta
        .new_errors
        .iter()
        .chain(delta.new_warnings.iter())
        .map(|k| k.stable())
        .collect();
    keel_output::github::annotations(
        result
            .errors
            .iter()
            .chain(result.warnings.iter())
            // Same key rule as the delta itself — reconstructing it inline
            // instead let every hash-less violation of one code in one file
            // match the first of them, so a single new W006 annotated all of
            // them as new.
            .filter(|v| {
                new_keys.contains(&keel_enforce::types::stable_identity(
                    &v.code, &v.hash, &v.file, v.line,
                ))
            }),
    )
}

/// Print a compile result and decide the exit code.
///
/// `github` swaps the chosen formatter for GitHub Actions workflow commands —
/// the annotations CI wants, from the binary, with no JSON post-processor in
/// the action. The clean-output contract is unchanged: zero violations means
/// empty stdout and exit 0 in every format.
fn output_result(
    formatter: &dyn OutputFormatter,
    result: &keel_enforce::types::CompileResult,
    strict: bool,
    verbose: bool,
    github: bool,
) -> i32 {
    let has_errors = !result.errors.is_empty();
    let has_warnings = !result.warnings.is_empty();

    if !has_errors && !has_warnings {
        if verbose {
            eprintln!("keel compile: clean — no violations");
        }
        return 0;
    }

    let output = if github {
        keel_output::github::annotations(result.errors.iter().chain(result.warnings.iter()))
    } else {
        formatter.format_compile(result)
    };
    if !output.is_empty() {
        println!("{}", output);
    }

    if has_errors || (strict && has_warnings) {
        1
    } else {
        0
    }
}

#[cfg(test)]
#[path = "compile_tests.rs"]
mod tests;
