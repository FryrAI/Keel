//! `keel quality` — the missing time axis.
//!
//! Every other keel surface answers a question about *now*: what does this diff
//! break, what is oversized today, which cycles exist. Maintainability decays
//! cumulatively and silently — each individual change was individually approved
//! — so a per-file gate cannot see it by construction. This module measures a
//! handful of countable properties of the persisted graph, stores the reading
//! against a commit, and reports the series.
//!
//! Three rules shape everything here:
//!
//! 1. **Countable, never a score.** Four metrics, each of which resolves to
//!    named instances a reader can go look at. No composite "quality score", no
//!    judgement about whether an abstraction is right.
//! 2. **Report, never a gate.** `keel quality` always exits 0. A ratio in the
//!    exit-code path is unactionable by construction — no file, no line, no
//!    hash, no fix.
//! 3. **Refuse rather than mislead.** The stored blob carries its own version;
//!    when a window spans two versions the comparison is refused, because a
//!    silently re-baselined series is a lie told in a straight line.
//!
//! Capture is decoupled from `keel map`: metrics are computed from the stored
//! graph on demand, so a `map --cached` (which rebuilds nothing) cannot inject
//! a duplicate point and the series stays smooth.

pub mod trend;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use keel_core::store::GraphStore;
use keel_core::types::QualityInputs;

use crate::audit::navigation::{find_cycles, is_reportable_cycle};
use crate::file_class::FileClass;
use crate::violations_economy::ENTRYPOINT_NAMES;

pub use trend::{MetricTrend, QualityPoint, QualityTrend, TrendStep};

/// Version of the metrics blob written into `quality_snapshots.metrics`.
///
/// Bump this whenever a metric's *definition* changes (not when one is added —
/// a new field with a serde default is backward-readable). A window that spans
/// two versions is refused by [`trend::build_trend`] rather than compared, so
/// bumping is cheap and honest, and quietly redefining a metric is impossible.
pub const METRICS_VERSION: u32 = 1;

/// The four countable readings taken from one graph state.
///
/// Serialized as the `metrics` blob of a `quality_snapshots` row. Field order
/// is the reporting order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Blob version — see [`METRICS_VERSION`].
    pub version: u32,
    /// Hand-written source files longer than the configured line budget
    /// (`enforce.max_file_lines`). Tests, generated clients and `.sql`/`.baml`
    /// surfaces are excluded, or the series tracks fixture growth rather than
    /// production decay.
    pub files_over_budget: u32,
    /// Module import/call cycles the audit would report — the same
    /// `circular_dep` population, Rust-only and over-long cycles excluded.
    pub cycle_count: u32,
    /// Private functions in hand-written source with no caller and no value
    /// use anywhere in the graph.
    pub dead_private_fns: u32,
    /// Share of dependency edges that leave their own file. Trend-only: there
    /// is no good absolute value, only a direction over time.
    pub cross_module_edge_ratio: f64,
}

impl QualityMetrics {
    /// The reportable metrics as `(name, value, judged)`, in reporting order.
    ///
    /// `judged` is false for `cross_module_edge_ratio`: a rising ratio may be
    /// decomposition or may be scatter, and keel does not know which, so it is
    /// reported as a direction and never as "worse".
    pub fn rows(&self) -> [(&'static str, f64, bool); 4] {
        [
            ("files_over_budget", self.files_over_budget as f64, true),
            ("cycle_count", self.cycle_count as f64, true),
            ("dead_private_fns", self.dead_private_fns as f64, true),
            (
                "cross_module_edge_ratio",
                self.cross_module_edge_ratio,
                false,
            ),
        ]
    }

    /// Serialize to the JSON blob stored in `quality_snapshots.metrics`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// One `keel quality` run's output: a current reading, a trend, or both.
///
/// `metrics` is `None` in `--trend` mode — the trend is assembled purely from
/// stored snapshots, so a trend report never needs to walk the graph.
#[derive(Debug, Clone, Serialize)]
pub struct QualityReport {
    /// keel version that produced the report.
    pub version: String,
    /// Always `"quality"` — the machine-readable command discriminator every
    /// keel result carries.
    pub command: String,
    /// `HEAD` at report time, when the working directory is a git checkout.
    pub commit: Option<String>,
    /// The current reading, computed from the stored graph.
    pub metrics: Option<QualityMetrics>,
    /// Whether this run persisted the reading as a snapshot (`--snapshot`).
    pub snapshot_saved: bool,
    /// The stored series (`--trend`).
    pub trend: Option<QualityTrend>,
}

impl QualityReport {
    /// An empty report carrying keel's version and the command name.
    pub fn new(commit: Option<String>) -> Self {
        QualityReport {
            version: env!("CARGO_PKG_VERSION").to_string(),
            command: "quality".to_string(),
            commit,
            metrics: None,
            snapshot_saved: false,
            trend: None,
        }
    }
}

/// Measure the four metrics against the persisted graph.
///
/// The store answers the whole measurement in four aggregate queries (see
/// [`keel_core::types::QualityInputs`]); this function applies the exemption
/// rules keel-enforce owns — file class, entrypoint names, reportable cycles —
/// and does no graph traversal of its own. That split is what keeps a full
/// measurement in the low milliseconds instead of one query per node.
///
/// `max_file_lines` is `enforce.max_file_lines` from `keel.json`, so the metric
/// tracks the budget the repo actually enforces via W007 rather than holding a
/// second, separate opinion about file size.
pub fn compute_metrics(store: &dyn GraphStore, max_file_lines: u32) -> QualityMetrics {
    metrics_from(&store.quality_inputs(), max_file_lines)
}

/// The pure half of [`compute_metrics`]: raw graph facts in, metrics out.
pub fn metrics_from(inputs: &QualityInputs, max_file_lines: u32) -> QualityMetrics {
    let files_over_budget = inputs
        .file_lines
        .iter()
        .filter(|(path, lines)| {
            *lines > max_file_lines && FileClass::classify(path).grades_size_and_naming()
        })
        .count() as u32;

    let dead_private_fns = inputs
        .uncalled_private_fns
        .iter()
        .filter(|(path, name)| {
            FileClass::classify(path).grades_size_and_naming() && !is_never_dead(name)
        })
        .count() as u32;

    let cycle_count = find_cycles(&module_dependency_graph(inputs))
        .iter()
        .filter(|c| is_reportable_cycle(c))
        .count() as u32;

    let cross_module_edge_ratio = if inputs.dependency_edges == 0 {
        0.0
    } else {
        // Two decimals: the series is read as a direction, and more precision
        // manufactures movement out of a single re-resolved edge.
        let raw = inputs.cross_file_dependency_edges as f64 / inputs.dependency_edges as f64;
        (raw * 100.0).round() / 100.0
    };

    QualityMetrics {
        version: METRICS_VERSION,
        files_over_budget,
        cycle_count,
        dead_private_fns,
        cross_module_edge_ratio,
    }
}

/// Names that carry no evidence of being dead however the graph reads.
///
/// Mirrors the naming exemptions W005 applies, minus the ones only a fresh
/// parse can see (decorators, trait context, `keel:keep`) — so this metric
/// over-counts relative to `keel compile` by construction. That is acceptable
/// for a *trend*, where the level is arbitrary and only the direction is read,
/// and it is stated here rather than discovered later.
fn is_never_dead(name: &str) -> bool {
    name.starts_with('_') || name.starts_with("bench_") || ENTRYPOINT_NAMES.contains(&name)
}

/// The module dependency graph, in the shape Tarjan's SCC wants.
///
/// Both endpoints of every dependency get a key: `find_cycles` skips neighbors
/// that are not keys, so a target file with no entry would hide the cycle it
/// closes.
fn module_dependency_graph(inputs: &QualityInputs) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
    for (path, _) in &inputs.file_lines {
        graph.entry(path.clone()).or_default();
    }
    for (source, target) in &inputs.module_deps {
        graph.entry(target.clone()).or_default();
        graph
            .entry(source.clone())
            .or_default()
            .insert(target.clone());
    }
    graph
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
