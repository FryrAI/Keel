//! Lightweight JSON string/number extraction without pulling in a full parser.
//!
//! Used by login, push, and upgrade commands for simple API response parsing.

/// Extract a JSON string value by key from a raw JSON string.
///
/// This is a simple substring search — it does NOT handle nested objects,
/// escaped quotes, or duplicate keys. Sufficient for flat API responses.
pub(crate) fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Extract a JSON unsigned integer value by key from a raw JSON string.
pub(crate) fn extract_json_number(json: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..num_end].parse().ok()
}

#[cfg(test)]
#[path = "json_helpers_tests.rs"]
mod tests;
