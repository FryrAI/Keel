//! Utility for detecting whether user input is a file path, function name, or hash.

/// Returns true if the string looks like a file path (contains separators or has a known extension).
pub fn looks_like_file_path(s: &str) -> bool {
    s.contains('/')
        || s.contains('\\')
        || s.ends_with(".py")
        || s.ends_with(".ts")
        || s.ends_with(".tsx")
        || s.ends_with(".js")
        || s.ends_with(".jsx")
        || s.ends_with(".go")
        || s.ends_with(".rs")
}

/// Returns true if the string matches the keel hash format (11 alphanumeric characters).
#[allow(dead_code)]
pub fn looks_like_hash(s: &str) -> bool {
    s.len() == 11 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Suggest a corrective hint if the input looks like it was intended for a different command.
pub fn suggest_command(input: &str) -> Option<String> {
    if looks_like_file_path(input) {
        Some(
            "Did you mean a file path? `keel discover` accepts file paths to list all symbols."
                .to_string(),
        )
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_file_path() {
        assert!(looks_like_file_path("src/main.rs"));
        assert!(looks_like_file_path("lib\\util.ts"));
        assert!(looks_like_file_path("handler.py"));
        assert!(looks_like_file_path("component.tsx"));
        assert!(!looks_like_file_path("aB3xZ9kLm2Q"));
        assert!(!looks_like_file_path("my_function"));
    }

    #[test]
    fn test_looks_like_hash() {
        assert!(looks_like_hash("aB3xZ9kLm2Q"));
        assert!(!looks_like_hash("short"));
        assert!(!looks_like_hash("waytoolongstring"));
        assert!(!looks_like_hash("aB3xZ9k!m2Q"));
    }

    #[test]
    fn test_suggest_command() {
        assert!(suggest_command("src/main.rs").is_some());
        assert!(suggest_command("aB3xZ9kLm2Q").is_none());
        assert!(suggest_command("my_function").is_none());
    }
}
