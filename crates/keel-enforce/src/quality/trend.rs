//! Assembling a trend from stored snapshots — including the refusal that keeps
//! it honest.
//!
//! The whole value of the series is that two points mean the same thing. When
//! they cannot (the metrics blob changed version between them, or a row is
//! unreadable), this module says so and reports no direction, rather than
//! drawing a line through two different definitions.

use serde::Serialize;

use keel_core::sqlite_quality::QualitySnapshotRow;

use super::{QualityMetrics, METRICS_VERSION};

/// One point of the series: a measurement and the commit it belongs to.
#[derive(Debug, Clone, Serialize)]
pub struct QualityPoint {
    /// The commit the measured graph described, when there was one.
    pub commit: Option<String>,
    /// Capture time, as stored (`YYYY-MM-DD HH:MM:SS`, UTC).
    pub captured_at: String,
    /// The reading.
    pub metrics: QualityMetrics,
}

impl QualityPoint {
    /// The commit abbreviated for display, or `"—"` when the snapshot carries
    /// no commit (captured outside a git checkout).
    pub fn short_commit(&self) -> String {
        match &self.commit {
            Some(sha) => sha.chars().take(7).collect(),
            None => "—".to_string(),
        }
    }
}

/// The single largest per-commit move in a metric — the "attribution" half of
/// the trend: which merge did this.
#[derive(Debug, Clone, Serialize)]
pub struct TrendStep {
    /// The commit whose snapshot recorded the move.
    pub commit: Option<String>,
    /// Capture time of that snapshot.
    pub captured_at: String,
    /// Change from the preceding point.
    pub delta: f64,
}

/// One metric's movement across the window.
#[derive(Debug, Clone, Serialize)]
pub struct MetricTrend {
    /// Metric name, as it appears in the stored blob.
    pub name: String,
    /// Value at the oldest point in the window.
    pub first: f64,
    /// Value at the newest point in the window.
    pub last: f64,
    /// `last - first`.
    pub delta: f64,
    /// `improving` / `worsening` / `flat` for the countable metrics; `up` /
    /// `down` / `flat` for the trend-only ratio, which keel does not judge.
    pub direction: String,
    /// Whether `direction` carries a value judgement.
    pub judged: bool,
    /// The largest single-commit move, when the window has more than one point.
    pub largest_step: Option<TrendStep>,
}

/// A trend over the stored series.
#[derive(Debug, Clone, Serialize)]
pub struct QualityTrend {
    /// Every usable point in the window, oldest → newest.
    pub points: Vec<QualityPoint>,
    /// Per-metric movement. Empty when the comparison was refused.
    pub metrics: Vec<MetricTrend>,
    /// Why the comparison was refused, when it was.
    pub refused: Option<String>,
    /// Rows in the window whose blob could not be parsed at all.
    pub unreadable: usize,
}

/// Build a trend from stored snapshot rows, oldest → newest.
///
/// Refuses — `refused` set, `metrics` empty — as soon as any row in the window
/// carries a metrics-blob version this binary does not define. That covers both
/// directions of drift: a window straddling a definition change, and a whole
/// series written by a newer keel. Points are still returned in both cases, so
/// the reader can see what exists and pick a narrower `--since`.
pub fn build_trend(rows: &[QualitySnapshotRow]) -> QualityTrend {
    let mut points = Vec::with_capacity(rows.len());
    let mut foreign_versions: Vec<u32> = Vec::new();
    let mut unreadable = 0usize;

    for row in rows {
        match blob_version(&row.metrics) {
            None => unreadable += 1,
            Some(v) if v != METRICS_VERSION => {
                if !foreign_versions.contains(&v) {
                    foreign_versions.push(v);
                }
            }
            Some(_) => match serde_json::from_str::<QualityMetrics>(&row.metrics) {
                Ok(metrics) => points.push(QualityPoint {
                    commit: row.commit_sha.clone(),
                    captured_at: row.captured_at.clone(),
                    metrics,
                }),
                Err(_) => unreadable += 1,
            },
        }
    }

    if !foreign_versions.is_empty() {
        foreign_versions.sort_unstable();
        let seen: Vec<String> = foreign_versions.iter().map(|v| format!("v{v}")).collect();
        return QualityTrend {
            points,
            metrics: Vec::new(),
            refused: Some(format!(
                "snapshots in this window were written under metrics {} while this keel \
                 defines v{} — the definitions differ, so comparing them would re-baseline \
                 the series silently. Narrow the window with `--since <sha>` to stay inside \
                 one version.",
                seen.join(", "),
                METRICS_VERSION,
            )),
            unreadable,
        };
    }

    let metrics = if points.len() < 2 {
        Vec::new()
    } else {
        metric_trends(&points)
    };

    QualityTrend {
        points,
        metrics,
        refused: None,
        unreadable,
    }
}

/// The blob's declared version, or `None` when it is not readable JSON with a
/// numeric `version`.
///
/// Probed separately from the full parse so a future blob whose *fields*
/// changed is reported as a version mismatch (refusable, explainable) instead
/// of as a corrupt row.
fn blob_version(blob: &str) -> Option<u32> {
    serde_json::from_str::<serde_json::Value>(blob)
        .ok()?
        .get("version")?
        .as_u64()
        .map(|v| v as u32)
}

/// Movement per metric across `points` (which must hold at least two).
fn metric_trends(points: &[QualityPoint]) -> Vec<MetricTrend> {
    let first_row = points[0].metrics.rows();
    let last_row = points[points.len() - 1].metrics.rows();

    first_row
        .iter()
        .enumerate()
        .map(|(i, (name, first, judged))| {
            let last = last_row[i].1;
            let delta = last - first;
            MetricTrend {
                name: (*name).to_string(),
                first: *first,
                last,
                delta,
                direction: direction(delta, *judged),
                judged: *judged,
                largest_step: largest_step(points, i),
            }
        })
        .collect()
}

/// Word for a delta. Lower is better for every judged metric, so a positive
/// delta is `worsening`; the unjudged ratio only gets a direction.
fn direction(delta: f64, judged: bool) -> String {
    let word = if delta.abs() < f64::EPSILON {
        "flat"
    } else if judged {
        if delta > 0.0 {
            "worsening"
        } else {
            "improving"
        }
    } else if delta > 0.0 {
        "up"
    } else {
        "down"
    };
    word.to_string()
}

/// The point that moved metric `index` the most relative to its predecessor.
///
/// This is the per-commit attribution: "`files_over_budget` +13 over three
/// weeks" is a fact, but "+6 of it at a1b2c3d" is where the reader goes to
/// look. Ties keep the earliest move, so the answer is stable across runs.
fn largest_step(points: &[QualityPoint], index: usize) -> Option<TrendStep> {
    let mut best: Option<TrendStep> = None;
    for pair in points.windows(2) {
        let delta = pair[1].metrics.rows()[index].1 - pair[0].metrics.rows()[index].1;
        if delta.abs() < f64::EPSILON {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|b| delta.abs() > b.delta.abs() + f64::EPSILON)
        {
            best = Some(TrendStep {
                commit: pair[1].commit.clone(),
                captured_at: pair[1].captured_at.clone(),
                delta,
            });
        }
    }
    best
}
