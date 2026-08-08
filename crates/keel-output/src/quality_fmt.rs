//! Rendering for `keel quality` — the current reading and the trend.
//!
//! Both the human and the LLM formatter live here rather than in their usual
//! homes because they share the one rule that must not drift: how a metric's
//! value is printed. `files_over_budget` is a count and `cross_module_edge_ratio`
//! is a ratio, and a series read as "0.34 → 0" because one renderer rounded
//! differently would be a fabricated regression.

use keel_enforce::quality::{MetricKind, MetricTrend, QualityMetrics, QualityReport, QualityTrend};

/// What to run when the series is empty — named once, printed by both formats.
const NO_SNAPSHOTS_HINT: &str =
    "no snapshots yet — run `keel quality --snapshot` (CI takes one per merge) to start the series";

/// Render one metric value: counts as integers, fractions to two decimals.
///
/// The precision rule follows the metric's own [`MetricKind`] rather than a
/// list of names kept here — a 0.34 reading printed as `0` would make the
/// series read as flat, and a renderer that has to remember to update its own
/// list is exactly how that happens.
fn value(kind: MetricKind, v: f64) -> String {
    match kind {
        MetricKind::Fraction => format!("{v:.2}"),
        MetricKind::Count => format!("{v:.0}"),
    }
}

/// Render a signed change, using the same precision rule as [`value`].
fn delta(kind: MetricKind, v: f64) -> String {
    let sign = if v > 0.0 { "+" } else { "" };
    format!("{sign}{}", value(kind, v))
}

/// Abbreviate a commit for display, or `—` when there is none.
fn short(commit: Option<&String>) -> String {
    match commit {
        Some(sha) => sha.chars().take(7).collect(),
        None => "—".to_string(),
    }
}

/// `keel quality` for people.
pub(crate) fn human(result: &QualityReport) -> String {
    let mut out = String::new();

    if let Some(metrics) = &result.metrics {
        out.push_str(&format!(
            "keel quality — {}\n",
            short(result.commit.as_ref())
        ));
        for row in metrics.rows() {
            out.push_str(&format!(
                "  {:<24} {}\n",
                row.name,
                value(row.kind, row.value)
            ));
        }
        if result.snapshot_saved {
            out.push_str("  (snapshot saved)\n");
        }
    }

    if let Some(trend) = &result.trend {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&human_trend(trend));
    }

    out
}

/// The trend table: movement per metric, then the raw per-commit series.
fn human_trend(trend: &QualityTrend) -> String {
    if let Some(reason) = &trend.refused {
        return format!("keel quality --trend: comparison refused\n  {reason}\n");
    }
    if trend.points.is_empty() {
        return format!("keel quality --trend: {NO_SNAPSHOTS_HINT}\n");
    }

    let first = &trend.points[0];
    let last = &trend.points[trend.points.len() - 1];
    let mut out = format!(
        "keel quality --trend — {} snapshot(s), {} → {}\n",
        trend.points.len(),
        short(first.commit.as_ref()),
        short(last.commit.as_ref()),
    );

    if trend.metrics.is_empty() {
        out.push_str("  one snapshot only — no direction yet\n");
    } else {
        out.push_str(&format!(
            "\n  {:<24} {:>7} {:>7} {:>9}  {}\n",
            "metric", "first", "last", "change", "direction"
        ));
        for m in &trend.metrics {
            out.push_str(&format!(
                "  {:<24} {:>7} {:>7} {:>9}  {}{}\n",
                m.name,
                value(m.kind, m.first),
                value(m.kind, m.last),
                delta(m.kind, m.delta),
                m.direction,
                attribution(m),
            ));
        }
    }

    out.push_str("\n  per-commit:\n");
    for p in &trend.points {
        // A metric this snapshot predates deserializes as 0.0. Printing that
        // would be the same fabricated reading the trend refuses to draw.
        let cells: Vec<String> = p
            .metrics
            .rows()
            .iter()
            .map(|row| match p.legacy_missing.contains(row.name) {
                true => format!("{}=—", row.name),
                false => format!("{}={}", row.name, value(row.kind, row.value)),
            })
            .collect();
        out.push_str(&format!(
            "    {}  {}  {}\n",
            short(p.commit.as_ref()),
            p.captured_at,
            cells.join(" "),
        ));
    }

    if !trend.omitted.is_empty() {
        out.push_str(&format!(
            "\n  not trended ({}): a snapshot in this window predates the metric\n",
            trend.omitted.join(", ")
        ));
    }

    if trend.unreadable > 0 {
        out.push_str(&format!(
            "\n  {} snapshot(s) skipped: unreadable metrics blob\n",
            trend.unreadable
        ));
    }

    out
}

/// ` (+6 at bbbbbbb)` — which commit moved this metric the most.
fn attribution(m: &MetricTrend) -> String {
    match &m.largest_step {
        Some(step) => format!(
            " ({} at {})",
            delta(m.kind, step.delta),
            short(step.commit.as_ref())
        ),
        None => String::new(),
    }
}

/// `keel quality --llm` — one line for the reading, one per metric for a trend.
pub(crate) fn llm(result: &QualityReport) -> String {
    let mut lines: Vec<String> = Vec::new();

    if let Some(metrics) = &result.metrics {
        lines.push(llm_reading(result, metrics));
    }

    if let Some(trend) = &result.trend {
        lines.extend(llm_trend(trend));
    }

    lines.join("\n")
}

/// `QUALITY commit=… files_over_budget=… …`
fn llm_reading(result: &QualityReport, metrics: &QualityMetrics) -> String {
    let mut parts = vec![format!("commit={}", short(result.commit.as_ref()))];
    for row in metrics.rows() {
        parts.push(format!("{}={}", row.name, value(row.kind, row.value)));
    }
    if result.snapshot_saved {
        parts.push("snapshot=saved".to_string());
    }
    format!("QUALITY {}", parts.join(" "))
}

/// The trend as one header line plus one line per metric.
fn llm_trend(trend: &QualityTrend) -> Vec<String> {
    if let Some(reason) = &trend.refused {
        return vec![
            format!(
                "QUALITY-TREND refused=version_mismatch points={}",
                trend.points.len()
            ),
            reason.clone(),
        ];
    }
    if trend.points.is_empty() {
        return vec![
            "QUALITY-TREND points=0".to_string(),
            format!("hint: {NO_SNAPSHOTS_HINT}"),
        ];
    }

    let first = &trend.points[0];
    let last = &trend.points[trend.points.len() - 1];
    let mut lines = vec![format!(
        "QUALITY-TREND points={} span={}..{}",
        trend.points.len(),
        short(first.commit.as_ref()),
        short(last.commit.as_ref()),
    )];
    for m in &trend.metrics {
        let step = match &m.largest_step {
            Some(s) => format!(
                " step={}@{}",
                delta(m.kind, s.delta),
                short(s.commit.as_ref())
            ),
            None => String::new(),
        };
        lines.push(format!(
            "{} {} -> {} ({} {}){}",
            m.name,
            value(m.kind, m.first),
            value(m.kind, m.last),
            delta(m.kind, m.delta),
            m.direction,
            step,
        ));
    }
    if !trend.omitted.is_empty() {
        lines.push(format!(
            "omitted={} reason=metric_not_in_every_snapshot",
            trend.omitted.join(",")
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::quality::{QualityPoint, TrendStep, METRICS_VERSION};
    use std::collections::HashSet;

    fn metrics(over: u32, ratio: f64) -> QualityMetrics {
        QualityMetrics {
            version: METRICS_VERSION,
            files_over_budget: over,
            cycle_count: 1,
            dead_private_fns: 2,
            cross_module_edge_ratio: ratio,
            high_cc_mass_share: 0.5,
            propagation_cost: 0.12,
            clone_loc_ratio: 0.07,
        }
    }

    fn report(metrics: Option<QualityMetrics>, trend: Option<QualityTrend>) -> QualityReport {
        let mut r = QualityReport::new(Some("abcdef1234567890".into()));
        r.metrics = metrics;
        r.trend = trend;
        r
    }

    fn point(sha: &str, m: QualityMetrics) -> QualityPoint {
        QualityPoint {
            commit: Some(sha.into()),
            captured_at: "2026-08-01 10:00:00".into(),
            metrics: m,
            legacy_missing: HashSet::new(),
        }
    }

    fn empty_trend() -> QualityTrend {
        QualityTrend {
            points: Vec::new(),
            metrics: Vec::new(),
            refused: None,
            unreadable: 0,
            omitted: Vec::new(),
        }
    }

    #[test]
    fn llm_reading_is_one_line_with_every_metric() {
        let out = llm(&report(Some(metrics(12, 0.34)), None));
        assert_eq!(out.lines().count(), 1);
        assert!(out.starts_with("QUALITY commit=abcdef1"));
        assert!(out.contains("files_over_budget=12"));
        assert!(out.contains("cross_module_edge_ratio=0.34"));
    }

    #[test]
    fn counts_never_render_as_decimals_and_the_ratio_always_does() {
        let out = llm(&report(Some(metrics(0, 0.0)), None));
        assert!(out.contains("files_over_budget=0"));
        assert!(out.contains("cross_module_edge_ratio=0.00"));
    }

    #[test]
    fn trend_lines_carry_direction_and_attribution() {
        let trend = QualityTrend {
            points: vec![
                point("aaaaaaa1", metrics(10, 0.30)),
                point("bbbbbbb2", metrics(18, 0.31)),
            ],
            metrics: vec![MetricTrend {
                name: "files_over_budget".into(),
                first: 10.0,
                last: 18.0,
                delta: 8.0,
                direction: "worsening".into(),
                judged: true,
                kind: MetricKind::Count,
                largest_step: Some(TrendStep {
                    commit: Some("bbbbbbb2".into()),
                    captured_at: "2026-08-01 10:00:00".into(),
                    delta: 8.0,
                }),
            }],
            refused: None,
            unreadable: 0,
            omitted: vec!["propagation_cost".to_string()],
        };
        let out = llm(&report(None, Some(trend)));
        assert!(out.contains("QUALITY-TREND points=2 span=aaaaaaa..bbbbbbb"));
        assert!(out.contains("files_over_budget 10 -> 18 (+8 worsening) step=+8@bbbbbbb"));
    }

    #[test]
    fn empty_series_says_how_to_start_one() {
        let out = human(&report(None, Some(empty_trend())));
        assert!(out.contains("keel quality --snapshot"));
    }

    #[test]
    fn refusal_is_printed_instead_of_a_direction() {
        let trend = QualityTrend {
            points: vec![point("aaaaaaa1", metrics(10, 0.3))],
            metrics: Vec::new(),
            refused: Some("metrics version changed".into()),
            unreadable: 0,
            omitted: Vec::new(),
        };
        let out = human(&report(None, Some(trend)));
        assert!(out.contains("comparison refused"));
        assert!(!out.contains("worsening"));
    }

    /// A metric a snapshot predates deserializes as `0.0`. Neither the trend
    /// table nor the per-commit series may print that as a reading.
    #[test]
    fn a_metric_a_snapshot_predates_renders_as_absent_not_as_zero() {
        let legacy = QualityPoint {
            legacy_missing: HashSet::from(["propagation_cost"]),
            ..point("aaaaaaa1", metrics(10, 0.30))
        };
        let trend = QualityTrend {
            points: vec![legacy, point("bbbbbbb2", metrics(12, 0.31))],
            metrics: Vec::new(),
            refused: None,
            unreadable: 0,
            omitted: vec!["propagation_cost".to_string()],
        };
        let out = human(&report(None, Some(trend)));
        assert!(out.contains("propagation_cost=—"));
        assert!(out.contains("propagation_cost=0.12"));
        assert!(out.contains("not trended (propagation_cost)"));
    }

    #[test]
    fn human_reading_names_the_snapshot_it_saved() {
        let mut r = report(Some(metrics(3, 0.2)), None);
        r.snapshot_saved = true;
        let out = human(&r);
        assert!(out.contains("keel quality — abcdef1"));
        assert!(out.contains("(snapshot saved)"));
    }
}
