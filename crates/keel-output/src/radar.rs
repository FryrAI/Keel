//! Visual audit display formatting (box-drawing with bars, grades, and findings).

use keel_enforce::types::{AuditResult, AuditSeverity};

// ── Constants ──────────────────────────────────────────────────────────────────

const INNER_W: usize = 78;

// ── Display helpers ────────────────────────────────────────────────────────────

fn compute_grade(total: u32, max: u32) -> &'static str {
    if max == 0 {
        return "N/A";
    }
    let pct = total as f64 / max as f64 * 100.0;
    if pct >= 95.0 {
        "A+"
    } else if pct >= 85.0 {
        "A"
    } else if pct >= 75.0 {
        "B+"
    } else if pct >= 65.0 {
        "B"
    } else if pct >= 50.0 {
        "C"
    } else if pct >= 35.0 {
        "D"
    } else {
        "F"
    }
}

fn format_bar(score: u32, max: u32, width: usize) -> String {
    if max == 0 {
        return "\u{2591}".repeat(width);
    }
    let filled = ((width as f64 * score as f64 / max as f64).round() as usize).min(width);
    format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(width - filled)
    )
}

fn dim_label(name: &str) -> &str {
    match name {
        "structure" => "Structure",
        "discoverability" => "Discoverability",
        "navigation" => "Navigation",
        "config" => "Agent Config",
        _ => name,
    }
}

fn center_str(text: &str, w: usize) -> String {
    let len = text.chars().count();
    if len >= w {
        return text.to_string();
    }
    let left = (w - len) / 2;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(w - len - left))
}

/// Word-wrap text to fit within `w` characters, splitting at word boundaries.
fn wrap_lines(text: &str, w: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            if word.len() > w {
                // Single word exceeds width — force-split
                let mut remaining = word;
                while remaining.len() > w {
                    lines.push(remaining[..w].to_string());
                    remaining = &remaining[w..];
                }
                current = remaining.to_string();
            } else {
                current = word.to_string();
            }
        } else if current.len() + 1 + word.len() > w {
            lines.push(current);
            current = word.to_string();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn box_top(w: usize) -> String {
    format!("\u{250c}{}\u{2510}\n", "\u{2500}".repeat(w))
}
fn box_sep(w: usize) -> String {
    format!("\u{251c}{}\u{2524}\n", "\u{2500}".repeat(w))
}
fn box_bot(w: usize) -> String {
    format!("\u{2514}{}\u{2518}\n", "\u{2500}".repeat(w))
}

fn box_row(content: &str, w: usize) -> String {
    let len = content.chars().count();
    if len >= w {
        let truncated: String = content.chars().take(w).collect();
        format!("\u{2502}{}\u{2502}\n", truncated)
    } else {
        format!("\u{2502}{}{}\u{2502}\n", content, " ".repeat(w - len))
    }
}

// ── Main display compositor ────────────────────────────────────────────────────

pub fn format_audit_display(result: &AuditResult) -> String {
    let w = INNER_W;
    let mut out = String::with_capacity(2048);

    // Header
    out.push_str(&box_top(w));
    out.push_str(&box_row(
        &center_str("keel audit \u{2014} AI Readiness", w),
        w,
    ));

    // Bars section
    out.push_str(&box_sep(w));
    for dim in &result.dimensions {
        let label = dim_label(&dim.name);
        let bar = format_bar(dim.score, dim.max_score, 20);
        let check = if dim.score == dim.max_score {
            "  \u{2713}"
        } else {
            "   "
        };
        let line = format!(
            "  {:<16}  {}  {}/{}{}",
            label, bar, dim.score, dim.max_score, check
        );
        out.push_str(&box_row(&line, w));
    }

    // Total + Grade
    out.push_str(&box_sep(w));
    let grade = compute_grade(result.total_score, result.max_score);
    let t_left = format!("  Total: {}/{}", result.total_score, result.max_score);
    let t_right = format!("Grade: {}  ", grade);
    let gap = w.saturating_sub(t_left.chars().count() + t_right.chars().count());
    out.push_str(&box_row(
        &format!("{}{}{}", t_left, " ".repeat(gap), t_right),
        w,
    ));

    // Findings (skip if none)
    let has_findings = result
        .dimensions
        .iter()
        .flat_map(|d| &d.findings)
        .any(|f| f.severity != AuditSeverity::Pass);

    if has_findings {
        out.push_str(&box_sep(w));
        for dim in &result.dimensions {
            for f in &dim.findings {
                if f.severity == AuditSeverity::Pass {
                    continue;
                }
                let tag = match f.severity {
                    AuditSeverity::Tip => "[TIP] ",
                    AuditSeverity::Warn => "[WARN]",
                    AuditSeverity::Fail => "[FAIL]",
                    AuditSeverity::Pass => continue,
                };
                let file_part = match &f.file {
                    Some(p) => format!("{}: ", p),
                    None => String::new(),
                };
                let prefix = format!("  {} {} \u{2014} ", tag, f.check);
                let body = format!("{}{}", file_part, f.message);
                let first_w = w.saturating_sub(prefix.chars().count());
                let body_lines = wrap_lines(&body, first_w);
                // First line with prefix
                out.push_str(&box_row(&format!("{}{}", prefix, body_lines[0]), w));
                // Continuation lines indented to align with body
                let indent = " ".repeat(prefix.chars().count());
                for cont in &body_lines[1..] {
                    out.push_str(&box_row(&format!("{}{}", indent, cont), w));
                }
                if let Some(ref tip) = f.tip {
                    let tip_prefix = "    Tip: ";
                    let tip_w = w.saturating_sub(tip_prefix.len());
                    let tip_lines = wrap_lines(tip, tip_w);
                    out.push_str(&box_row(&format!("{}{}", tip_prefix, tip_lines[0]), w));
                    let tip_indent = " ".repeat(tip_prefix.len());
                    for cont in &tip_lines[1..] {
                        out.push_str(&box_row(&format!("{}{}", tip_indent, cont), w));
                    }
                }
            }
        }
    }

    out.push_str(&box_bot(w));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_grade() {
        assert_eq!(compute_grade(20, 20), "A+");
        assert_eq!(compute_grade(18, 20), "A");
        assert_eq!(compute_grade(15, 20), "B+");
        assert_eq!(compute_grade(13, 20), "B");
        assert_eq!(compute_grade(10, 20), "C");
        assert_eq!(compute_grade(7, 20), "D");
        assert_eq!(compute_grade(4, 20), "F");
        assert_eq!(compute_grade(0, 0), "N/A");
    }

    #[test]
    fn test_format_bar() {
        let bar = format_bar(5, 5, 20);
        assert_eq!(bar.chars().count(), 20);
        assert!(bar.chars().all(|c| c == '\u{2588}'));

        let bar = format_bar(0, 5, 20);
        assert_eq!(bar.chars().count(), 20);
        assert!(bar.chars().all(|c| c == '\u{2591}'));
    }

    #[test]
    fn test_wrap_lines_short() {
        let lines = wrap_lines("short text", 40);
        assert_eq!(lines, vec!["short text"]);
    }

    #[test]
    fn test_wrap_lines_long() {
        let lines = wrap_lines("this is a longer piece of text that should wrap", 20);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.len() <= 20, "line too long: {}", line);
        }
    }

    #[test]
    fn test_wrap_lines_empty() {
        let lines = wrap_lines("", 40);
        assert_eq!(lines, vec![""]);
    }
}
