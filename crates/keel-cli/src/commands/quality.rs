//! `keel quality` — measure the persisted graph, remember the reading, and
//! report the series.
//!
//! Thin wrapper over the shared [`keel_enforce::quality`] core: this file owns
//! the store handle, the git commit, and the exit contract, and nothing else.

use keel_enforce::quality::{compute_metrics, trend::build_trend, QualityReport};
use keel_output::OutputFormatter;

/// Arguments for `keel quality`, mirroring the clap subcommand.
#[derive(Debug, Default)]
pub struct QualityArgs {
    /// Capture the current reading as a snapshot against `HEAD`.
    pub snapshot: bool,
    /// Report the stored series instead of the current reading.
    pub trend: bool,
    /// Start the trend at this commit (accepts a short prefix).
    pub since: Option<String>,
    /// Cap the trend to the last N snapshots.
    pub last: Option<usize>,
    /// Export the stored snapshot history as JSON Lines to this path.
    pub export: Option<String>,
    /// Import snapshot history from this JSON Lines file, upserting by
    /// `commit_sha`.
    pub import: Option<String>,
}

/// Run `keel quality`.
///
/// **Exit 0 always**, whatever the numbers say — the report is the product, per
/// the `validate-plan` precedent. A ratio in the exit-code path is unactionable
/// by construction: no file, no line, no hash, no fix. Only an internal failure
/// (no initialized graph, an unwritable database) exits 2, per the CLI
/// contract.
///
/// `--export`/`--import` are their own runs (mutually exclusive with a
/// reading or a trend, enforced by clap): they move the `quality_snapshots`
/// series to/from a JSON Lines file so CI can accumulate history across a
/// per-commit graph cache that never carries a prior commit's row on its own.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, args: QualityArgs) -> i32 {
    let ctx = match super::open_repo("quality") {
        Ok(x) => x,
        Err(code) => return code,
    };
    let (cwd, store) = (ctx.cwd, ctx.store);

    if let Some(path) = &args.export {
        let jsonl = keel_enforce::quality::history::export_jsonl(&store);
        if let Err(e) = std::fs::write(path, &jsonl) {
            eprintln!("keel quality: failed to write {}: {}", path, e);
            return 2;
        }
        return 0;
    }
    if let Some(path) = &args.import {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("keel quality: failed to read {}: {}", path, e);
                return 2;
            }
        };
        return match keel_enforce::quality::history::import_jsonl(&store, &content) {
            Ok(summary) => {
                if verbose {
                    eprintln!(
                        "keel quality: imported {} snapshot(s) ({} new, {} updated)",
                        summary.total(),
                        summary.inserted,
                        summary.updated
                    );
                }
                0
            }
            Err(e) => {
                eprintln!("keel quality: failed to import {}: {}", path, e);
                2
            }
        };
    }

    let commit = keel_enforce::gitdiff::head_commit(&cwd);
    let mut report = QualityReport::new(commit.clone());

    if args.trend {
        let rows = match &args.since {
            Some(sha) => {
                let rows = store.quality_snapshots_since(sha);
                if rows.is_empty() {
                    // Not an internal error: the graph is fine, the ref just
                    // has no snapshot. Say which, and stay at exit 0.
                    eprintln!(
                        "keel quality: no snapshot at commit {sha} — \
                         run `keel quality --trend` to see the commits that have one"
                    );
                }
                rows
            }
            None => store.quality_snapshots(args.last.unwrap_or(0)),
        };
        report.trend = Some(build_trend(&rows));
    } else {
        let config = keel_core::config::KeelConfig::load(&ctx.keel_dir);
        let metrics = compute_metrics(&store, config.enforce.max_file_lines);
        report.metrics = Some(metrics);

        if args.snapshot {
            match store.insert_quality_snapshot(commit.as_deref(), &metrics.to_json()) {
                Ok(write) => {
                    report.snapshot_saved = true;
                    if verbose {
                        eprintln!("keel quality: snapshot {write:?}");
                    }
                }
                Err(e) => {
                    eprintln!("keel quality: failed to store snapshot: {e}");
                    return 2;
                }
            }
        }
    }

    let rendered = formatter.format_quality(&report);
    if !rendered.is_empty() {
        println!("{}", rendered);
    }

    0
}
