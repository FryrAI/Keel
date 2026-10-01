//! AST literal folding and fixed-text extraction for supported whole-file grammars.

use super::decode::{brace_segments, decode, printf_segments};
use tree_sitter::Node;

/// Read a syntax node's exact source slice.
pub(super) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

/// Named non-comment children, in source order.
pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
        .collect()
}

/// Literal kinds supported by the four whole-file grammars.
pub(super) fn is_literal(kind: &str) -> bool {
    matches!(
        kind,
        "string_literal"
            | "raw_string_literal"
            | "string"
            | "template_string"
            | "interpreted_string_literal"
            | "concatenated_string"
    )
}

/// Whether the node invokes Rust's unqualified `concat!` macro.
pub(super) fn is_concat(node: Node<'_>, source: &str) -> bool {
    node.kind() == "macro_invocation"
        && node
            .child_by_field_name("macro")
            .is_some_and(|n| text(n, source) == "concat")
}

fn delimiters<'a>(node: Node<'_>, source: &'a str, lang: &str) -> Option<(&'a str, bool, bool)> {
    let value = text(node, source);
    let i = value.find(['"', '\'', '`'])?;
    let prefix = value[..i].to_ascii_lowercase();
    let quote = value.as_bytes()[i] as char;
    let width = if lang == "python" && value[i..].starts_with(&quote.to_string().repeat(3)) {
        3
    } else {
        1
    };
    let end = if lang == "rust" && prefix.contains('r') {
        value.rfind('"')?
    } else {
        value.len().checked_sub(width)?
    };
    Some((
        &value[i + width..end],
        prefix.contains('r') || quote == '`' && lang == "go",
        prefix.contains('b'),
    ))
}

/// Splits interpolated Python/JS literals at AST boundaries before decoding.
fn ast_segments(node: Node<'_>, source: &str, lang: &str) -> Option<Vec<String>> {
    let (body, raw, byte) = delimiters(node, source, lang)?;
    let start = body.as_ptr() as usize - source.as_ptr() as usize;
    let mut offset = start;
    let mut pieces = Vec::new();
    let fstring = lang == "python"
        && text(node, source)
            .split(['"', '\''])
            .next()?
            .to_ascii_lowercase()
            .contains('f');
    let cooked = |piece: &str| {
        // Collapse source-level brace escapes before decoding: braces produced
        // by Unicode/hex escapes are already literal characters at runtime.
        let piece = if fstring {
            piece.replace("{{", "{").replace("}}", "}")
        } else {
            piece.to_string()
        };
        decode(&piece, lang, raw || byte)
    };
    for child in children(node) {
        if matches!(child.kind(), "interpolation" | "template_substitution") {
            pieces.push(cooked(&source[offset..child.start_byte()])?);
            offset = child.end_byte();
        }
    }
    pieces.push(cooked(&source[offset..start + body.len()])?);
    Some(pieces)
}

/// Decode a literal, folding Rust concat and Python adjacent literals.
pub(super) fn literal_segments(node: Node<'_>, source: &str, lang: &str) -> Option<Vec<String>> {
    if is_concat(node, source) {
        let tree = children(node)
            .into_iter()
            .find(|n| n.kind() == "token_tree")?;
        // concat! supports more than strings in Rust, but only string-only
        // concatenation is eligible here; rejecting other args avoids guessing.
        let args = children(tree);
        if args.is_empty() || args.iter().any(|n| !is_literal(n.kind())) {
            return None;
        }
        let mut joined = String::new();
        for arg in args {
            joined.push_str(&literal_segments(arg, source, lang)?.join("\0"));
        }
        return Some(vec![joined]);
    }
    if node.kind() == "concatenated_string" {
        let mut pieces = vec![String::new()];
        for child in children(node) {
            let next = literal_segments(child, source, lang)?;
            pieces.last_mut()?.push_str(next.first()?);
            pieces.extend(next.into_iter().skip(1));
        }
        return Some(pieces);
    }
    if matches!(lang, "python" | "typescript") {
        return ast_segments(node, source, lang);
    }
    let (body, raw, byte) = delimiters(node, source, lang)?;
    Some(vec![decode(body, lang, raw || byte)?])
}

/// Extract fixed pieces from an eligible returned template expression.
pub(super) fn template_segments(
    node: Node<'_>,
    source: &str,
    lang: &str,
    fmt: bool,
) -> Option<Vec<String>> {
    if node.kind() == "parenthesized_expression" {
        let parts = children(node);
        let [expression] = parts.as_slice() else {
            return None;
        };
        return template_segments(*expression, source, lang, fmt);
    }
    if is_literal(node.kind()) || is_concat(node, source) {
        return literal_segments(node, source, lang);
    }
    formatted_literal(node, source, lang, fmt, true).map(|(_, pieces)| pieces)
}

/// Recognize format literals once, so home and occurrence cooking agree.
pub(super) fn formatted_literal<'a>(
    node: Node<'a>,
    source: &str,
    lang: &str,
    fmt: bool,
    home: bool,
) -> Option<(Node<'a>, Vec<String>)> {
    if lang == "rust" && node.kind() == "macro_invocation" {
        let name = text(node.child_by_field_name("macro")?, source);
        if name != "format"
            && (home
                || !matches!(
                    name,
                    "format_args"
                        | "print"
                        | "println"
                        | "eprint"
                        | "eprintln"
                        | "write"
                        | "writeln"
                        | "panic"
                ))
        {
            return None;
        }
        let tree = children(node)
            .into_iter()
            .find(|n| n.kind() == "token_tree")?;
        let args = children(tree);
        let first = if matches!(name, "write" | "writeln") {
            // The destination may contain several token-tree nodes.
            args.into_iter().find(|n| is_literal(n.kind()))?
        } else {
            *args.first()?
        };
        if !is_literal(first.kind()) {
            return None;
        }
        return Some((
            first,
            brace_segments(&literal_segments(first, source, lang)?.join("\0"))?,
        ));
    }
    if lang == "python" && node.kind() == "call" {
        let function = node.child_by_field_name("function")?;
        if function.kind() != "attribute"
            || text(function.child_by_field_name("attribute")?, source) != "format"
        {
            return None;
        }
        let object = function.child_by_field_name("object")?;
        if !is_literal(object.kind()) {
            return None;
        }
        return Some((
            object,
            brace_segments(&literal_segments(object, source, lang)?.join("\0"))?,
        ));
    }
    if lang == "go" && fmt && node.kind() == "call_expression" {
        let function = node.child_by_field_name("function")?;
        if function.kind() != "selector_expression" {
            return None;
        }
        if text(function.child_by_field_name("operand")?, source) != "fmt"
            || text(function.child_by_field_name("field")?, source) != "Sprintf"
        {
            return None;
        }
        let arg = children(node.child_by_field_name("arguments")?)
            .into_iter()
            .next()?;
        if !is_literal(arg.kind()) {
            return None;
        }
        return Some((
            arg,
            printf_segments(&literal_segments(arg, source, lang)?.join("\0"))?,
        ));
    }
    None
}
