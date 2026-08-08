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
//! 1. **Countable, never a score.** Seven metrics, each of which resolves to
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

pub mod history;
pub mod trend;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use keel_core::store::GraphStore;
use keel_core::types::QualityInputs;

use crate::audit::navigation::{find_cycles, is_reportable_cycle};
use crate::file_class::FileClass;
use crate::violations_economy::is_exempt_dead_name;

pub use trend::{MetricTrend, QualityPoint, QualityTrend, TrendStep};

/// Version of the metrics blob written into `quality_snapshots.metrics`.
///
/// Bump this whenever a metric's *definition* changes (not when one is added —
/// a new field with a serde default is backward-readable). A window that spans
/// two versions is refused by [`trend::build_trend`] rather than compared, so
/// bumping is cheap and honest, and quietly redefining a metric is impossible.
///
/// Version 1 covers the definitions as they ship in 0.5.0: pre-release
/// refinements (the `dead_private_fns` exemptions, say) are part of settling
/// what version 1 *means*, since no released keel has written a snapshot yet.
pub const METRICS_VERSION: u32 = 1;

/// The seven countable readings taken from one graph state.
///
/// Serialized as the `metrics` blob of a `quality_snapshots` row. Field order
/// is the reporting order.
///
/// Fields added after version 1 shipped carry `#[serde(default)]` and must be
/// named in `trend::METRICS_ADDED_AFTER_V1`, or a blob predating them would
/// deserialize as `0.0` and the trend would draw a step out of "not measured".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Blob version — see [`METRICS_VERSION`].
    pub version: u32,
    /// Hand-written source files longer than the configured line budget
    /// (`enforce.max_file_lines`). Tests, generated clients and `.sql`/`.baml`
    /// surfaces are excluded, or the series tracks fixture growth rather than
    /// production decay.
    ///
    /// Shares W007's budget but deliberately not W007's population: the metric
    /// exempts by [`FileClass`], which also drops generated and declarative
    /// files, while W007 exempts only test and stub files. It also has no
    /// "and it grew" gate — a file that is over budget counts on every reading,
    /// which is what makes the series a level rather than a change log. So the
    /// two counts can legitimately differ on one tree.
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
    /// Share of the repo's complexity-weighted mass (`Σ cc·√sloc`) held by
    /// functions above `HIGH_CC_THRESHOLD`. A rising share means branching is
    /// concentrating into a shrinking set of hot functions — the shape erosion
    /// takes when line counts stay flat. Judged: higher is worse.
    ///
    /// Population: hand-written, non-test-context functions — the same rule
    /// [`Self::clone_loc_ratio`] uses. Test *files* are dropped by
    /// [`FileClass`], inline `#[cfg(test)]` modules by the persisted
    /// `in_test_context` flag.
    #[serde(default)]
    pub high_cc_mass_share: f64,
    /// Share of module pairs where one transitively reaches the other
    /// through calls/imports — `reachable_pairs / n²` over the module
    /// dependency graph. Trend-only: a rise may be shared infrastructure
    /// growing or coupling spreading, and keel cannot tell those apart.
    #[serde(default)]
    pub propagation_cost: f64,
    /// Share of function-body code lines that sit under a token window
    /// repeated in a *different* function — fragment-level (jscpd-style)
    /// duplication, measured by [`keel_core::fragments`] during `keel map`.
    ///
    /// Trend-only, and deliberately so: a match is a token shape, so a rise
    /// may be genuine copy-paste or may be a family of functions converging on
    /// one dispatch idiom. Both numerator and denominator count only
    /// lines carrying code, in hand-written source, so the ratio is
    /// interpretable on its own scale — but the level is not a target, only
    /// its direction is readable.
    #[serde(default)]
    pub clone_loc_ratio: f64,
}

/// Cyclomatic complexity above which a function's mass counts as "hot" for
/// [`QualityMetrics::high_cc_mass_share`].
///
/// Hardcoded, like the audit's cycle cap: a configurable threshold would let a
/// repo tune the metric until the series flattens, which is the one thing a
/// trend must not permit.
const HIGH_CC_THRESHOLD: u32 = 10;

impl QualityMetrics {
    /// The reportable metrics as `(name, value, judged)`, in reporting order.
    ///
    /// `judged` is false for `cross_module_edge_ratio`, `propagation_cost` and
    /// `clone_loc_ratio`: a rise in the first two may be decomposition or may
    /// be scatter, and a rise in the third may be copy-paste or may be a
    /// shared idiom. keel does not know which, so each is reported as a
    /// direction and never as "worse".
    pub fn rows(&self) -> [(&'static str, f64, bool); 7] {
        [
            ("files_over_budget", self.files_over_budget as f64, true),
            ("cycle_count", self.cycle_count as f64, true),
            ("dead_private_fns", self.dead_private_fns as f64, true),
            (
                "cross_module_edge_ratio",
                self.cross_module_edge_ratio,
                false,
            ),
            ("high_cc_mass_share", self.high_cc_mass_share, true),
            ("propagation_cost", self.propagation_cost, false),
            ("clone_loc_ratio", self.clone_loc_ratio, false),
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

/// Measure the seven metrics against the persisted graph.
///
/// The store answers the whole measurement in five aggregate queries (see
/// [`keel_core::types::QualityInputs`]); this function applies the exemption
/// rules keel-enforce owns — file class, entrypoint names, reportable cycles —
/// and does no graph traversal of its own. That split is what keeps a full
/// measurement in the low milliseconds instead of one query per node.
///
/// `max_file_lines` is `enforce.max_file_lines` from `keel.json`, so the metric
/// reads the budget the repo actually enforces via W007 rather than holding a
/// second, separate opinion about how long a file may be. The *population* it
/// applies that budget to is its own — see [`QualityMetrics::files_over_budget`].
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

    // `is_exempt_dead_name` is exactly W005's name-only exemption set. The
    // exemptions only a fresh parse can see (decorators, trait context,
    // `keel:keep`, test context) are missing here by construction, so this
    // metric over-counts relative to `keel compile` — acceptable for a *trend*,
    // where the level is arbitrary and only the direction is read.
    let dead_private_fns = inputs
        .uncalled_private_fns
        .iter()
        .filter(|(path, name)| {
            FileClass::classify(path).grades_size_and_naming() && !is_exempt_dead_name(name)
        })
        .count() as u32;

    // One graph, two readings — cycles and reachability walk the same edges.
    let graph = module_dependency_graph(inputs);
    let cycle_count = find_cycles(&graph)
        .iter()
        .filter(|c| is_reportable_cycle(c))
        .count() as u32;

    let cross_module_edge_ratio = ratio(
        inputs.cross_file_dependency_edges as f64,
        inputs.dependency_edges as f64,
    );

    // Weighting by √sloc rather than by sloc keeps a single 400-line switch
    // from dominating the share, while still ranking a long hot function above
    // a short one.
    let (hot_mass, total_mass) = inputs
        .complexity_by_fn
        .iter()
        .filter(|(path, _, _)| FileClass::classify(path).grades_size_and_naming())
        .fold((0.0f64, 0.0f64), |(hot, total), (_, cc, sloc)| {
            let mass = *cc as f64 * (*sloc as f64).sqrt();
            let hot = if *cc > HIGH_CC_THRESHOLD {
                hot + mass
            } else {
                hot
            };
            (hot, total + mass)
        });
    let high_cc_mass_share = ratio(hot_mass, total_mass);

    // The scan already excluded everything that is not hand-written source, but
    // the rows are read back from a database that an older map may have
    // written, so the class filter is applied again here rather than trusted.
    let (cloned_lines, code_lines) = inputs
        .fragment_clones_by_fn
        .iter()
        .filter(|(path, _, _)| FileClass::classify(path).grades_size_and_naming())
        .fold((0.0f64, 0.0f64), |(cloned, total), (_, c, l)| {
            (cloned + *c as f64, total + *l as f64)
        });

    QualityMetrics {
        version: METRICS_VERSION,
        files_over_budget,
        cycle_count,
        dead_private_fns,
        cross_module_edge_ratio,
        high_cc_mass_share,
        propagation_cost: propagation_cost(&graph),
        clone_loc_ratio: ratio(cloned_lines, code_lines),
    }
}

/// `part / whole` rounded to two decimals; 0 when `whole` is 0.
///
/// Two decimals because the series is read as a direction, and more precision
/// manufactures movement out of a single re-resolved edge.
fn ratio(part: f64, whole: f64) -> f64 {
    if whole == 0.0 {
        return 0.0;
    }
    (part / whole * 100.0).round() / 100.0
}

/// `reachable_pairs / n²`: for every module, how many *other* modules it can
/// transitively reach via calls/imports, summed over modules.
///
/// Self-pairs are excluded from the numerator — a module trivially "reaches"
/// itself, and that is not propagation. Unjudged: a rise may be legitimate
/// shared infrastructure or spreading coupling, the same stance
/// `cross_module_edge_ratio` takes.
///
/// O(n·(V+E)): a DFS per module. Fine at the hundreds-to-low-thousands of
/// files `keel quality`'s budget targets.
fn propagation_cost(graph: &HashMap<String, HashSet<String>>) -> f64 {
    let n = graph.len();
    if n == 0 {
        return 0.0;
    }
    let reachable_pairs: usize = graph
        .keys()
        .map(|start| {
            let mut seen: HashSet<&str> = HashSet::new();
            let mut stack: Vec<&str> = vec![start.as_str()];
            while let Some(module) = stack.pop() {
                for next in graph.get(module).into_iter().flatten() {
                    if next != start && seen.insert(next.as_str()) {
                        stack.push(next.as_str());
                    }
                }
            }
            seen.len()
        })
        .sum();
    ratio(reachable_pairs as f64, (n * n) as f64)
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
