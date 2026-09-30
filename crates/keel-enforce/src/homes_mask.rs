//! Delete syntax-tree comments while preserving source line numbers.

use std::borrow::Cow;
use std::path::Path;

use keel_parsers::treesitter::{detect_language, TreeSitterParser};

/// Remove comment bytes except newlines for languages with whole-file grammars.
pub(crate) fn without_comments<'a>(path: &str, text: &'a str) -> Cow<'a, str> {
    let Some(lang @ ("rust" | "python" | "go" | "typescript" | "javascript" | "tsx" | "bash")) =
        detect_language(Path::new(path))
    else {
        return Cow::Borrowed(text);
    };
    let Ok(tree) = TreeSitterParser::new().parse(lang, text.as_bytes()) else {
        return Cow::Borrowed(text);
    };
    let mut cursor = tree.walk();
    let mut out = String::new();
    let mut end = 0;
    loop {
        let node = cursor.node();
        if node.kind().contains("comment") || node.kind() == "hash_bang_line" {
            out.push_str(&text[end..node.start_byte()]);
            out.extend(text[node.byte_range()].chars().filter(|&c| c == '\n'));
            end = node.end_byte();
        } else if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                if end == 0 {
                    return Cow::Borrowed(text);
                }
                out.push_str(&text[end..]);
                return Cow::Owned(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homes_mask_multiline_nested_unicode_comments_preserve_lines() {
        let source = "/* äußere\n /* CURRENT_DATE */\n ende */\nlet x = \"CURRENT_DATE\";\n";
        assert_eq!(
            without_comments("q.rs", source),
            "\n\n\nlet x = \"CURRENT_DATE\";\n"
        );
    }

    #[test]
    fn homes_mask_large_file_comments_and_parse_cost() {
        let source = "// CURRENT_DATE\nconst SQL: &str = \"CURRENT_DATE\";\n".repeat(2_000);
        let expected = "\nconst SQL: &str = \"CURRENT_DATE\";\n".repeat(2_000);
        let start = std::time::Instant::now();
        for _ in 0..10 {
            assert_eq!(without_comments("q.rs", &source), expected);
        }
        eprintln!(
            "homes masking: 10 parses of {} bytes in {:?}",
            source.len(),
            start.elapsed()
        );
    }
}
