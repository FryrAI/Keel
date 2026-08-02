//! Rendering for `keel stats` — the graph totals and the telemetry rollup.
//!
//! Both the human and the LLM rendering live here (the `quality_fmt`
//! precedent): the two formats read the same numbers, and a count that
//! disagrees between them would be a fabricated difference.
//!
//! Neither renderer ends its output with a newline — the caller's `println!`
//! supplies it.

use keel_enforce::types::{StatsResult, TelemetryAggregate};

/// `keel stats` for people: counts, the per-kind edge breakdown, and the
/// telemetry section when there is one.
pub(crate) fn human(result: &StatsResult) -> String {
    let mut lines = vec![
        "keel stats".to_string(),
        format!("  modules:   {}", result.modules),
        format!("  functions: {}", result.functions),
        format!("  files:     {}", result.files),
        format!("  edges:     {}", result.edges),
        format!("    calls:    {}", result.calls_edges),
        format!("    imports:  {}", result.imports_edges),
        format!("    contains: {}", result.contains_edges),
        format!("    uses:     {}", result.uses_edges),
    ];

    if let Some(path) = &result.db_path {
        lines.push(format!("  db_path:   {}", path));
    }
    if let Some(v) = result.schema_version {
        lines.push(format!("  schema:    v{}", v));
    }

    if let Some(agg) = &result.telemetry {
        lines.push(String::new());
        push_telemetry_human(&mut lines, agg);
    }

    lines.join("\n")
}

/// `keel stats --llm`: one `STATS` line, plus the `TELEMETRY` line when the
/// project has recorded anything.
pub(crate) fn llm(result: &StatsResult) -> String {
    let mut lines = vec![format!(
        "STATS modules={} functions={} files={} edges={}",
        result.modules, result.functions, result.files, result.edges
    )];
    if let Some(agg) = &result.telemetry {
        lines.push(telemetry_llm_line(agg));
    }
    lines.join("\n")
}

/// Compact `TELEMETRY key=value ...` line for `keel stats --llm` — the
/// standing regression guard for T1.1: `compile_p50_ms`/`compile_p95_ms`
/// surface a re-introduced network round trip on the compile hot path.
fn telemetry_llm_line(agg: &TelemetryAggregate) -> String {
    let mut parts = vec![format!("invocations={}", agg.total_invocations)];
    if let Some(v) = agg.avg_compile_ms {
        parts.push(format!("avg_compile_ms={}", v as u64));
    }
    if let Some(v) = agg.compile_p50_ms {
        parts.push(format!("compile_p50_ms={}", v as u64));
    }
    if let Some(v) = agg.compile_p95_ms {
        parts.push(format!("compile_p95_ms={}", v as u64));
    }
    if let Some(v) = agg.avg_map_ms {
        parts.push(format!("avg_map_ms={}", v as u64));
    }
    parts.push(format!("errors={}", agg.total_errors));
    parts.push(format!("warnings={}", agg.total_warnings));
    format!("TELEMETRY {}", parts.join(" "))
}

/// The human telemetry section: the 30-day rollup and agent adoption.
fn push_telemetry_human(lines: &mut Vec<String>, agg: &TelemetryAggregate) {
    lines.push("  telemetry (last 30 days):".to_string());
    lines.push(format!("    invocations: {}", agg.total_invocations));
    if let Some(avg) = agg.avg_compile_ms {
        lines.push(format!("    avg compile:  {}ms", avg as u64));
    }
    if agg.compile_p50_ms.is_some() || agg.compile_p95_ms.is_some() {
        lines.push(format!(
            "    compile p50/p95: {}ms / {}ms",
            agg.compile_p50_ms.map(|v| v as u64).unwrap_or(0),
            agg.compile_p95_ms.map(|v| v as u64).unwrap_or(0)
        ));
    }
    if let Some(avg) = agg.avg_map_ms {
        let formatted = if avg >= 1000.0 {
            format!("{:.1}s", avg / 1000.0)
        } else {
            format!("{}ms", avg as u64)
        };
        lines.push(format!("    avg map:      {}", formatted));
    }
    lines.push(format!("    errors:       {}", agg.total_errors));
    lines.push(format!("    warnings:     {}", agg.total_warnings));

    if !agg.command_counts.is_empty() {
        lines.push(format!(
            "    top commands: {}",
            top_counts(&agg.command_counts)
        ));
    }

    if !agg.language_percentages.is_empty() {
        let mut langs: Vec<_> = agg.language_percentages.iter().collect();
        langs.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        let lang_str: Vec<String> = langs
            .iter()
            .map(|(k, v)| format!("{} {:.0}%", k, v))
            .collect();
        lines.push(format!("    languages:    {}", lang_str.join(", ")));
    }

    if !agg.top_error_codes.is_empty() {
        lines.push(format!(
            "    top errors:   {}",
            top_counts(&agg.top_error_codes)
        ));
    }

    if !agg.agent_stats.is_empty() {
        lines.push(String::new());
        lines.push("    agent adoption:".to_string());
        let mut agents: Vec<_> = agg.agent_stats.iter().collect();
        agents.sort_by_key(|a| std::cmp::Reverse(a.1.sessions));
        for (name, stats) in agents {
            lines.push(format!(
                "      {}: {} sessions, avg {:.0} tool calls/session",
                name, stats.sessions, stats.avg_tool_calls_per_session
            ));
            if !stats.tool_usage.is_empty() {
                lines.push(format!(
                    "        top tools: {}",
                    top_counts(&stats.tool_usage)
                ));
            }
        }
    }
}

/// `name (count), ...` for the five biggest entries — the one shape every
/// count histogram in the telemetry section is printed with.
fn top_counts(counts: &std::collections::HashMap<String, u64>) -> String {
    let mut sorted: Vec<_> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    sorted
        .iter()
        .take(5)
        .map(|(k, v)| format!("{} ({})", k, v))
        .collect::<Vec<_>>()
        .join(", ")
}
