/// Token budget estimation and truncation for LLM output.
///
/// Estimates output size in tokens (approximation: 1 token ≈ 4 chars)
/// and truncates when exceeding budget.
const CHARS_PER_TOKEN: usize = 4;

/// Estimate token count from a string.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(CHARS_PER_TOKEN)
}

/// Truncate a list of formatted items to fit within a token budget.
///
/// Truncation happens at item granularity: each element is kept or dropped
/// whole, and `overflow_count` is the number of trailing items dropped. Pass
/// one item per unit you want the "+N more" trailer to count (e.g. one item
/// per violation, not one per file). At least one item is always kept.
/// Returns (kept_items, overflow_count).
pub fn truncate_to_budget(lines: &[String], max_tokens: usize) -> (Vec<String>, usize) {
    let mut kept = Vec::new();
    let mut total_chars = 0;
    let max_chars = max_tokens * CHARS_PER_TOKEN;

    for (i, line) in lines.iter().enumerate() {
        let line_chars = line.len() + 1; // +1 for newline
        if total_chars + line_chars > max_chars && !kept.is_empty() {
            return (kept, lines.len() - i);
        }
        total_chars += line_chars;
        kept.push(line.clone());
    }

    (kept, 0)
}

/// Render an always-kept `header` followed by entry blocks, dropping trailing
/// whole entries to fit `max_tokens` and appending a `... +N more (raise
/// --budget)` trailer when any are dropped.
///
/// Each entry is a self-contained block (it may span multiple lines) so
/// truncation never splits an entry. Used by `skeleton` and `focus` LLM output.
pub fn render_with_budget(header: &str, entries: &[String], max_tokens: usize) -> String {
    let remaining = max_tokens.saturating_sub(estimate_tokens(header)).max(1);
    let (kept, overflow) = truncate_to_budget(entries, remaining);

    let mut out = String::from(header);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for entry in &kept {
        out.push_str(entry);
        if !entry.ends_with('\n') {
            out.push('\n');
        }
    }
    if overflow > 0 {
        out.push_str(&format!("... +{} more (raise --budget)\n", overflow));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hi"), 1);
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars / 4 = 2.75 -> 3
    }

    #[test]
    fn test_truncate_fits() {
        let lines = vec!["line1".into(), "line2".into()];
        let (kept, overflow) = truncate_to_budget(&lines, 100);
        assert_eq!(kept.len(), 2);
        assert_eq!(overflow, 0);
    }

    #[test]
    fn test_truncate_over_budget() {
        let lines: Vec<String> = (0..20)
            .map(|i| format!("violation {} with long description text here", i))
            .collect();
        let (kept, overflow) = truncate_to_budget(&lines, 50);
        assert!(kept.len() < 20);
        assert!(overflow > 0);
        assert_eq!(kept.len() + overflow, 20);
    }

    #[test]
    fn test_truncate_keeps_at_least_one() {
        let lines = vec![
            "a very long line that exceeds budget alone".into(),
            "second".into(),
        ];
        let (kept, _overflow) = truncate_to_budget(&lines, 1);
        assert!(!kept.is_empty()); // Always keeps at least one
    }

    #[test]
    fn test_render_with_budget_fits() {
        let entries = vec!["fn a()".into(), "fn b()".into()];
        let out = render_with_budget("HEADER", &entries, 500);
        assert!(out.starts_with("HEADER\n"));
        assert!(out.contains("fn a()"));
        assert!(out.contains("fn b()"));
        assert!(!out.contains("more (raise --budget)"));
    }

    #[test]
    fn test_render_with_budget_truncates_with_trailer() {
        let entries: Vec<String> = (0..20)
            .map(|i| format!("fn function_number_{i}(arg: SomeLongType) -> ResultType"))
            .collect();
        let out = render_with_budget("HEADER symbols=20", &entries, 30);
        assert!(out.contains("more (raise --budget)"));
        // Header is always present even under a tiny budget.
        assert!(out.starts_with("HEADER symbols=20\n"));
        // Complete entries only: every kept line is a full entry (no partials).
        for line in out.lines().skip(1) {
            if line.contains("more (raise --budget)") {
                continue;
            }
            assert!(line.starts_with("fn function_number_"));
        }
    }
}
