//! GitHub Actions workflow-command output.
//!
//! Emits `::error file=…,line=…,title=[CODE]::message` lines, which GitHub
//! renders as inline annotations on the diff of a pull request.
//!
//! This exists in the binary because the alternative was a `python3` heredoc
//! inside `.github/actions/keel/action.yml` reshaping keel's JSON — an
//! undeclared runtime dependency in a product whose Article 1 and Principle 10
//! are both "single binary, zero runtime dependencies".
//!
//! Format reference: <https://docs.github.com/actions/reference/workflow-commands-for-github-actions>

use keel_enforce::types::Violation;

/// Escape a workflow-command *message* (everything after `::`).
///
/// GitHub reads a raw newline as the end of the command, so a multi-line
/// message silently truncates the annotation and leaks its tail into the log.
fn escape_data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Escape a workflow-command *property* value (`file=…`, `title=…`).
///
/// Properties are comma-separated `k=v` pairs, so `,` and `:` must be escaped
/// on top of the message rules or a filename containing either would split the
/// annotation across the wrong fields.
fn escape_property(s: &str) -> String {
    escape_data(s).replace(':', "%3A").replace(',', "%2C")
}

/// `error` for anything GitHub should fail loudly on, `warning` otherwise.
///
/// keel severities are `ERROR`, `WARNING`, and `INFO`; `INFO` (a suppressed
/// S001) maps to `notice`, which annotates without colouring the check red.
fn level(severity: &str) -> &'static str {
    match severity {
        "ERROR" => "error",
        "INFO" => "notice",
        _ => "warning",
    }
}

/// One annotation line for one violation.
pub fn annotation(v: &Violation) -> String {
    format!(
        "::{} file={},line={},title=[{}]::{}",
        level(&v.severity),
        escape_property(&v.file),
        v.line.max(1),
        escape_property(&v.code),
        escape_data(&v.message),
    )
}

/// Annotations for a whole violation set, newline-separated and unterminated.
///
/// Returns the empty string when there is nothing to annotate, so callers can
/// honor keel's clean-output contract by printing only a non-empty result.
pub fn annotations<'a>(violations: impl IntoIterator<Item = &'a Violation>) -> String {
    violations
        .into_iter()
        .map(annotation)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violation(code: &str, severity: &str, file: &str, line: u32, message: &str) -> Violation {
        Violation {
            code: code.into(),
            severity: severity.into(),
            category: "test".into(),
            message: message.into(),
            file: file.into(),
            line,
            hash: "abc12345678".into(),
            confidence: 1.0,
            resolution_tier: "tier1".into(),
            fix_hint: None,
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        }
    }

    #[test]
    fn golden_error_line() {
        let v = violation(
            "E003",
            "ERROR",
            "src/lib.rs",
            42,
            "Public function `run` has no docstring",
        );
        assert_eq!(
            annotation(&v),
            "::error file=src/lib.rs,line=42,title=[E003]::Public function `run` has no docstring"
        );
    }

    #[test]
    fn warnings_and_suppressions_get_their_own_levels() {
        let w = violation("W005", "WARNING", "src/a.rs", 7, "no callers");
        assert!(w_line_is(&w, "::warning"), "{}", annotation(&w));
        let s = violation("S001", "INFO", "src/a.rs", 7, "suppressed");
        assert!(w_line_is(&s, "::notice"), "{}", annotation(&s));
    }

    fn w_line_is(v: &Violation, prefix: &str) -> bool {
        annotation(v).starts_with(prefix)
    }

    #[test]
    fn a_multiline_message_stays_on_one_annotation() {
        let v = violation("W006", "WARNING", "src/a.rs", 3, "identical to\n`other`");
        let line = annotation(&v);
        assert_eq!(line.lines().count(), 1);
        assert!(line.ends_with("identical to%0A`other`"), "{line}");
    }

    #[test]
    fn commas_and_colons_in_a_path_cannot_split_the_properties() {
        let v = violation("E002", "ERROR", "src/od,d:name.rs", 1, "x");
        assert!(
            annotation(&v).contains("file=src/od%2Cd%3Aname.rs,line=1,"),
            "{}",
            annotation(&v)
        );
    }

    #[test]
    fn line_zero_becomes_line_one() {
        // GitHub drops an annotation whose line is 0.
        let v = violation("W007", "WARNING", "src/a.rs", 0, "too big");
        assert!(annotation(&v).contains(",line=1,"));
    }

    #[test]
    fn an_empty_set_annotates_nothing() {
        assert_eq!(annotations(&[]), "");
    }
}
