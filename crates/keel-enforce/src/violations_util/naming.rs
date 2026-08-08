/// Split `name` into (start, end) byte ranges for each "word" segment, using
/// snake_case underscores if present, else camelCase upper-case transitions.
fn segment_ranges(name: &str) -> Vec<(usize, usize)> {
    if name.contains('_') {
        let mut ranges = Vec::new();
        let mut start = 0;
        for (i, c) in name.char_indices() {
            if c == '_' {
                if i > start {
                    ranges.push((start, i));
                }
                start = i + c.len_utf8();
            }
        }
        if start < name.len() {
            ranges.push((start, name.len()));
        }
        return ranges;
    }

    // camelCase, with acronym runs treated as one segment: a new segment
    // starts at an uppercase char only when the previous char is lowercase
    // (a plain lower->upper transition, e.g. "parse"|"Header"), or when the
    // previous char is ALSO uppercase but the next char is lowercase (this
    // is the last capital of a run, which kicks off the next word, e.g. the
    // second "H" in "HTTPHeader" starts "Header" while "HTTP" stays whole).
    // Standard camel tokenization: "parseHTTPHeader" -> parse/HTTP/Header,
    // "toJSONString" -> to/JSON/String, "readIOBuffer" -> read/IO/Buffer.
    // Without this, consecutive capitals would shatter into one-char
    // segments ("parseHTTPHeader" -> p-a-r-s-e-H-T-T-P... over-matching
    // even worse than a single leading segment).
    let mut ranges = Vec::new();
    let mut start = 0;
    let indices: Vec<(usize, char)> = name.char_indices().collect();
    for i in 1..indices.len() {
        let (idx, ch) = indices[i];
        if !ch.is_uppercase() {
            continue;
        }
        let prev_is_upper = indices[i - 1].1.is_uppercase();
        let next_is_lower = indices.get(i + 1).is_some_and(|&(_, c)| c.is_lowercase());
        if !prev_is_upper || next_is_lower {
            ranges.push((start, idx));
            start = idx;
        }
    }
    if !indices.is_empty() {
        ranges.push((start, name.len()));
    }
    ranges
}

/// Extract a multi-segment name prefix (e.g. "get_user" from "get_user_name",
/// "getUser" from "getUserName") used to suggest a placement module.
///
/// Only a prefix spanning **at least two** segments is meaningful enough to
/// suggest a placement — a single leading segment (e.g. "make" from
/// "make_relative") over-matches wildly, since unrelated functions across
/// the whole codebase commonly share a first word. Names with fewer than
/// three total segments can't produce a two-segment prefix while leaving a
/// remainder (a single word, or a two-word name where the "prefix" would be
/// the entire name), so they return an empty prefix and never fire W001.
pub fn extract_prefix(name: &str) -> String {
    let ranges = segment_ranges(name);
    if ranges.len() < 3 {
        return String::new();
    }
    name[ranges[0].0..ranges[1].1].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_prefix() {
        assert_eq!(extract_prefix("x"), "");
    }

    #[test]
    fn test_extract_prefix_all_lowercase() {
        assert_eq!(extract_prefix("process"), "");
    }

    #[test]
    fn test_extract_prefix_single_segment_names_never_match() {
        // Common short verbs/names — never enough segments to produce a prefix.
        assert_eq!(extract_prefix("run"), "");
        assert_eq!(extract_prefix("main"), "");
        assert_eq!(extract_prefix("default"), "");
    }

    #[test]
    fn test_extract_prefix_snake_case_multi() {
        // 3+ segments: prefix is the first TWO segments, not just the first.
        assert_eq!(extract_prefix("get_user_name"), "get_user");
        assert_eq!(extract_prefix("get_user_profile_data"), "get_user");
    }

    #[test]
    fn test_extract_prefix_camel_case_multi() {
        assert_eq!(extract_prefix("getUserName"), "getUser");
        // Only two segments — no match, same rule as snake_case.
        assert_eq!(extract_prefix("handleRequest"), "");
    }

    #[test]
    fn test_extract_prefix_acronym_runs_stay_one_segment() {
        // Consecutive capitals are one segment (the acronym), not one
        // segment per letter — otherwise "parseHTTPHeader" would shatter
        // into p/a/r/s/e/H/T/T/P/... and over-match worse than the old
        // single-segment behavior. Standard camel tokenization: the run
        // splits right before its last capital when that capital is
        // followed by a lowercase letter (it belongs to the next word).
        assert_eq!(extract_prefix("parseHTTPHeader"), "parseHTTP");
        assert_eq!(extract_prefix("toJSONString"), "toJSON");
        assert_eq!(extract_prefix("readIOBuffer"), "readIO");

        // All-caps name: one giant acronym run = a single segment = no
        // multi-segment prefix possible.
        assert_eq!(extract_prefix("HTTP"), "");
    }

    #[test]
    fn test_extract_prefix_two_segment_names_never_match() {
        // Regression guard: these used to extract a single-segment prefix
        // ("make", "process") that over-matched wildly. Now they need a
        // third segment to leave room for a genuine 2-segment prefix.
        assert_eq!(extract_prefix("make_relative"), "");
        assert_eq!(extract_prefix("process_order"), "");
    }
}
