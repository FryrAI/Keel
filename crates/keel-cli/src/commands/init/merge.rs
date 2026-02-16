//! Merge strategies for tool config files.
//!
//! Two strategies:
//! 1. JSON deep merge — for settings.json / hooks.json files
//! 2. Markdown marker merge — for instruction files with `<!-- keel:start/end -->` markers

use std::fs;
use std::path::Path;

/// Deep-merge two JSON values. For objects, keys are merged recursively.
/// For arrays under a "hooks" key, items are appended if the command string
/// is not already present. Otherwise, the new value wins.
pub fn json_deep_merge(base: &serde_json::Value, overlay: &serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            let mut merged = base_map.clone();
            for (key, overlay_val) in overlay_map {
                if let Some(base_val) = base_map.get(key) {
                    merged.insert(key.clone(), json_deep_merge(base_val, overlay_val));
                } else {
                    merged.insert(key.clone(), overlay_val.clone());
                }
            }
            serde_json::Value::Object(merged)
        }
        (serde_json::Value::Array(base_arr), serde_json::Value::Array(overlay_arr)) => {
            let mut merged = base_arr.clone();
            for item in overlay_arr {
                if !array_contains_command(&merged, item) {
                    merged.push(item.clone());
                }
            }
            serde_json::Value::Array(merged)
        }
        // For all other types, overlay wins
        (_, overlay_val) => overlay_val.clone(),
    }
}

/// Check if an array already contains an item with the same "command" string.
fn array_contains_command(arr: &[serde_json::Value], item: &serde_json::Value) -> bool {
    let item_cmd = item
        .as_object()
        .and_then(|o| o.get("command"))
        .and_then(|v| v.as_str());

    if let Some(cmd) = item_cmd {
        arr.iter().any(|existing| {
            existing
                .as_object()
                .and_then(|o| o.get("command"))
                .and_then(|v| v.as_str())
                == Some(cmd)
        })
    } else {
        // No command field — check for exact equality
        arr.contains(item)
    }
}

/// Merge a JSON template into an existing file, or write it fresh.
/// Returns the final content string.
pub fn merge_json_file(target: &Path, template: &str) -> Result<String, String> {
    let template_val: serde_json::Value = serde_json::from_str(template)
        .map_err(|e| format!("invalid template JSON: {}", e))?;

    if target.exists() {
        let existing = fs::read_to_string(target)
            .map_err(|e| format!("failed to read {}: {}", target.display(), e))?;
        let existing_val: serde_json::Value = serde_json::from_str(&existing)
            .map_err(|e| format!("invalid JSON in {}: {}", target.display(), e))?;
        let merged = json_deep_merge(&existing_val, &template_val);
        serde_json::to_string_pretty(&merged)
            .map_err(|e| format!("failed to serialize merged JSON: {}", e))
    } else {
        serde_json::to_string_pretty(&template_val)
            .map_err(|e| format!("failed to serialize template JSON: {}", e))
    }
}

/// Merge a markdown template using `<!-- keel:start -->` / `<!-- keel:end -->` markers.
///
/// - If target exists and has markers: replace the section between markers
/// - If target exists but no markers: append the template content
/// - If target doesn't exist: return the template as-is
pub fn merge_markdown_file(target: &Path, template: &str) -> Result<String, String> {
    if !target.exists() {
        return Ok(template.to_string());
    }

    let existing = fs::read_to_string(target)
        .map_err(|e| format!("failed to read {}: {}", target.display(), e))?;

    if let Some(result) = replace_marker_section(&existing, template) {
        Ok(result)
    } else {
        // No markers found — append
        let mut result = existing;
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push('\n');
        result.push_str(template);
        Ok(result)
    }
}

/// Replace content between `<!-- keel:start -->` and `<!-- keel:end -->` markers.
/// Returns None if markers are not found.
fn replace_marker_section(existing: &str, replacement: &str) -> Option<String> {
    let start_marker = "<!-- keel:start -->";
    let end_marker = "<!-- keel:end -->";

    let start_idx = existing.find(start_marker)?;
    let end_idx = existing.find(end_marker)?;

    if end_idx <= start_idx {
        return None;
    }

    let end_of_end_marker = end_idx + end_marker.len();
    // Consume the trailing newline after end marker if present
    let end_of_end_marker = if existing[end_of_end_marker..].starts_with('\n') {
        end_of_end_marker + 1
    } else {
        end_of_end_marker
    };

    let mut result = String::new();
    result.push_str(&existing[..start_idx]);
    result.push_str(replacement);
    if !replacement.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&existing[end_of_end_marker..]);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_deep_merge_disjoint_keys() {
        let base: serde_json::Value = serde_json::json!({"a": 1});
        let overlay: serde_json::Value = serde_json::json!({"b": 2});
        let merged = json_deep_merge(&base, &overlay);
        assert_eq!(merged, serde_json::json!({"a": 1, "b": 2}));
    }

    #[test]
    fn test_json_deep_merge_nested_objects() {
        let base: serde_json::Value = serde_json::json!({"hooks": {"a": [1]}});
        let overlay: serde_json::Value = serde_json::json!({"hooks": {"b": [2]}});
        let merged = json_deep_merge(&base, &overlay);
        assert_eq!(merged, serde_json::json!({"hooks": {"a": [1], "b": [2]}}));
    }

    #[test]
    fn test_json_deep_merge_preserves_existing_key() {
        let base: serde_json::Value = serde_json::json!({"existing_key": true});
        let overlay: serde_json::Value = serde_json::json!({"hooks": {}});
        let merged = json_deep_merge(&base, &overlay);
        assert_eq!(merged["existing_key"], serde_json::json!(true));
    }

    #[test]
    fn test_replace_marker_section() {
        let existing = "# My file\n<!-- keel:start -->\nold content\n<!-- keel:end -->\nfooter\n";
        let replacement = "<!-- keel:start -->\nnew content\n<!-- keel:end -->\n";
        let result = replace_marker_section(existing, replacement).unwrap();
        assert!(result.contains("new content"));
        assert!(!result.contains("old content"));
        assert!(result.contains("footer"));
        assert!(result.contains("# My file"));
    }

    #[test]
    fn test_replace_marker_section_no_markers() {
        let existing = "# My file\nsome content\n";
        let replacement = "<!-- keel:start -->\nnew\n<!-- keel:end -->\n";
        assert!(replace_marker_section(existing, replacement).is_none());
    }
}
