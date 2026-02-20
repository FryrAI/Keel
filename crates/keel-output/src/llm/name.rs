use keel_enforce::types::NameResult;

pub fn format_name(result: &NameResult) -> String {
    if result.suggestions.is_empty() {
        return format!("NAME no suggestions for \"{}\"\n", result.description,);
    }

    let best = &result.suggestions[0];
    let mut out = format!("NAME suggestion for \"{}\"\n", result.description,);

    out.push_str(&format!(
        "\nLOCATION {} (best match: [{}] score={:.2})\n",
        best.location,
        best.keywords.join(","),
        best.score,
    ));

    for alt in &best.alternatives {
        out.push_str(&format!(
            "  ALT {} ([{}] score={:.2})\n",
            alt.location,
            alt.keywords.join(","),
            alt.score,
        ));
    }

    if let (Some(after), Some(line)) = (&best.insert_after, best.insert_line) {
        out.push_str(&format!(
            "INSERT after {} (line {}) — same responsibility cluster\n",
            after, line,
        ));
    }

    out.push_str(&format!(
        "CONVENTION {} (matches module style)\n",
        best.convention,
    ));
    out.push_str(&format!("SUGGESTED {}\n", best.suggested_name));

    if !best.likely_imports.is_empty() {
        out.push_str(&format!(
            "IMPORTS likely: {} (used by siblings)\n",
            best.likely_imports.join(", "),
        ));
    }

    if !best.siblings.is_empty() {
        out.push_str(&format!("SIBLINGS {}\n", best.siblings.join(", ")));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::*;

    #[test]
    fn test_empty_name() {
        let result = NameResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "name".into(),
            description: "validate JWT token".into(),
            suggestions: vec![],
        };
        assert!(format_name(&result).contains("no suggestions"));
    }

    #[test]
    fn test_name_with_suggestion() {
        let result = NameResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "name".into(),
            description: "validate JWT token and check expiry".into(),
            suggestions: vec![NameSuggestion {
                location: "src/auth/validation.rs".into(),
                score: 0.92,
                keywords: vec!["auth".into(), "jwt".into(), "validation".into()],
                alternatives: vec![NameAlternative {
                    location: "src/auth/middleware.rs".into(),
                    score: 0.71,
                    keywords: vec!["auth".into(), "middleware".into()],
                }],
                insert_after: Some("validate_token".into()),
                insert_line: Some(45),
                convention: "snake_case, prefix: validate_".into(),
                suggested_name: "validate_jwt_expiry".into(),
                likely_imports: vec!["jsonwebtoken::decode".into(), "chrono::Utc".into()],
                siblings: vec!["validate_token".into(), "validate_session".into()],
            }],
        };
        let out = format_name(&result);
        assert!(out.contains("LOCATION src/auth/validation.rs"));
        assert!(out.contains("score=0.92"));
        assert!(out.contains("ALT src/auth/middleware.rs"));
        assert!(out.contains("INSERT after validate_token"));
        assert!(out.contains("SUGGESTED validate_jwt_expiry"));
        assert!(out.contains("IMPORTS likely:"));
        assert!(out.contains("SIBLINGS"));
    }
}
