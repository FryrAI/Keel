/// Token budget estimation and truncation for LLM output.
///
/// Estimates output size in tokens (approximation: 1 token ≈ 4 chars)
/// and truncates when exceeding budget.

const CHARS_PER_TOKEN: usize = 4;

/// Estimate token count from a string.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN
}

/// Truncate a list of formatted lines to fit within a token budget.
/// Returns (kept_lines, overflow_count).
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
        let lines: Vec<String> = (0..20).map(|i| format!("violation {} with long description text here", i)).collect();
        let (kept, overflow) = truncate_to_budget(&lines, 50);
        assert!(kept.len() < 20);
        assert!(overflow > 0);
        assert_eq!(kept.len() + overflow, 20);
    }

    #[test]
    fn test_truncate_keeps_at_least_one() {
        let lines = vec!["a very long line that exceeds budget alone".into(), "second".into()];
        let (kept, _overflow) = truncate_to_budget(&lines, 1);
        assert!(kept.len() >= 1); // Always keeps at least one
    }
}
