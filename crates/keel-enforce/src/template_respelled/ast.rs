//! Pure template body eligibility and literal traversal.

use super::literals::{children, is_concat, is_literal, literal_segments, template_segments, text};
use super::{eligible_segment, MIN_SEGMENT_CHARS};
use keel_core::template_homes::TemplateHome;
use keel_parsers::treesitter::{detect_language, in_test_context, TreeSitterParser};
use std::path::Path;
use tree_sitter::Node;

/// Current home expression bounds, recalculated even when mapped lines drift.
pub(super) struct HomeSpan {
    pub home: TemplateHome,
    pub start: usize,
    pub end: usize,
}

/// One decoded literal or supported concatenation at its source position.
pub(super) struct Literal {
    pub text: String,
    pub pieces: Vec<String>,
    pub line: u32,
    pub start: usize,
    pub end: usize,
}

/// Select the whole-file grammar population specified by design v2.
pub(super) fn language(file: &str) -> Option<&'static str> {
    match detect_language(Path::new(file))? {
        "rust" => Some("rust"),
        "python" => Some("python"),
        "go" => Some("go"),
        "typescript" | "javascript" => Some("typescript"),
        "tsx" | "jsx" => Some("tsx"),
        _ => None,
    }
}

fn callable(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "method_declaration"
    )
}

fn only_expression<'a>(node: Node<'a>, lang: &str) -> Option<Node<'a>> {
    let body = node.child_by_field_name("body")?;
    if !matches!(body.kind(), "block" | "statement_block") {
        return (node.kind() == "arrow_function").then_some(body);
    }
    let mut statements = children(body);
    if lang == "go" && statements.len() == 1 && statements[0].kind() == "statement_list" {
        statements = children(statements[0]);
    }
    let [mut expr] = statements.as_slice() else {
        return None;
    };
    if expr.kind() == "expression_statement" {
        // A Rust expression ending in ';' returns (), unless it is `return`.
        if lang == "rust" && expr.named_child(0)?.kind() != "return_expression" {
            return None;
        }
        expr = expr.named_child(0)?;
    }
    if matches!(expr.kind(), "return_statement" | "return_expression") {
        let parts = children(expr);
        let [value] = parts.as_slice() else {
            return None;
        };
        expr = *value;
        if expr.kind() == "expression_list" {
            let parts = children(expr);
            let [value] = parts.as_slice() else {
                return None;
            };
            expr = *value;
        }
    } else if lang != "rust" {
        return None;
    }
    while expr.kind() == "parenthesized_expression" {
        let parts = children(expr);
        let [value] = parts.as_slice() else {
            return None;
        };
        expr = *value;
    }
    Some(expr)
}

fn walk(node: Node<'_>, visit: &mut impl FnMut(Node<'_>)) {
    visit(node);
    for child in children(node) {
        walk(child, visit);
    }
}

/// Conservatively refuse Sprintf if `fmt` is bound anywhere in the file. This
/// admits the exact standard import and avoids mistaking a shadow for std fmt.
fn std_fmt(root: Node<'_>, source: &str) -> bool {
    let mut imported = false;
    let mut shadowed = false;
    walk(root, &mut |node| {
        if node.kind() == "import_spec" {
            if node
                .child_by_field_name("path")
                .is_some_and(|p| text(p, source) == "\"fmt\"")
            {
                imported |= node
                    .child_by_field_name("name")
                    .is_none_or(|n| text(n, source) == "fmt");
            }
        } else if matches!(
            node.kind(),
            "parameter_declaration"
                | "variadic_parameter_declaration"
                | "var_spec"
                | "const_spec"
                | "short_var_declaration"
                | "range_clause"
                | "function_declaration"
                | "type_spec"
        ) {
            let mut cursor = node.walk();
            let bindings: Vec<_> = node.children_by_field_name("name", &mut cursor).collect();
            for binding in bindings.into_iter().chain(node.child_by_field_name("left")) {
                walk(binding, &mut |n| {
                    if n.kind() == "identifier" && text(n, source) == "fmt" {
                        shadowed = true;
                    }
                });
            }
        }
    });
    imported && !shadowed
}

/// Scan current homes and non-test literals using the source grammar.
pub(super) fn scan(file: &str, source: &str) -> (Vec<HomeSpan>, Vec<Literal>) {
    if crate::file_class::FileClass::classify(file) == crate::file_class::FileClass::Test
        || crate::violations_util::is_test_file(file)
    {
        return (vec![], vec![]);
    }
    let Some(parse_lang) = language(file) else {
        return (vec![], vec![]);
    };
    let lang = if parse_lang == "tsx" {
        "typescript"
    } else {
        parse_lang
    };
    let mut parser = TreeSitterParser::new();
    let Ok(tree) = parser.parse(parse_lang, source.as_bytes()) else {
        return (vec![], vec![]);
    };
    let root = tree.root_node();
    let definitions = parser
        .parse_file(parse_lang, Path::new(file), source)
        .map(|r| r.definitions)
        .unwrap_or_default();
    let fmt = lang == "go" && std_fmt(root, source);
    let mut homes = Vec::new();
    walk(root, &mut |node| {
        if !callable(node.kind())
            || node.has_error()
            || in_test_context(node, parse_lang, source.as_bytes())
        {
            return;
        }
        let Some(expr) = only_expression(node, lang) else {
            return;
        };
        let Some(pieces) = template_segments(expr, source, lang, fmt) else {
            return;
        };
        let def = definitions.iter().find(|d| {
            d.line_start <= node.start_position().row as u32 + 1
                && d.line_end == node.end_position().row as u32 + 1
                && d.kind == keel_core::types::NodeKind::Function
                && node
                    .child_by_field_name("body")
                    .is_some_and(|body| d.body_text == text(body, source))
        });
        let name = def.map(|d| d.name.clone()).or_else(|| {
            node.child_by_field_name("name")
                .map(|n| text(n, source).to_string())
        });
        let Some(name) = name else {
            return;
        };
        let mut segments: Vec<_> = pieces
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| eligible_segment(s, MIN_SEGMENT_CHARS))
            .collect();
        segments.sort();
        segments.dedup();
        homes.push(HomeSpan {
            home: TemplateHome {
                hash: def.map(|d| d.hash()).unwrap_or_else(|| {
                    keel_core::hash::compute_hash(&name, text(node, source), "")
                }),
                name,
                file: file.into(),
                line: node.start_position().row as u32 + 1,
                segments,
            },
            start: expr.start_byte(),
            end: expr.end_byte(),
        });
    });
    let mut literals = Vec::new();
    collect_literals(root, source, parse_lang, lang, &mut literals);
    (homes, literals)
}

fn collect_literals(
    node: Node<'_>,
    source: &str,
    parse_lang: &str,
    lang: &str,
    literals: &mut Vec<Literal>,
) {
    if in_test_context(node, parse_lang, source.as_bytes()) {
        return;
    }
    if is_literal(node.kind()) || is_concat(node, source) {
        if !node.has_error() {
            if let Some(pieces) = literal_segments(node, source, lang) {
                literals.push(Literal {
                    text: pieces.join("\0"),
                    pieces,
                    line: node.start_position().row as u32 + 1,
                    start: node.start_byte(),
                    end: node.end_byte(),
                });
            }
        }
        // Folded children must not produce duplicate occurrences. Strings
        // *inside* interpolation expressions remain independent literals.
        interpolations(node, source, parse_lang, lang, literals);
        return;
    }
    for child in children(node) {
        collect_literals(child, source, parse_lang, lang, literals);
    }
}

fn interpolations(
    node: Node<'_>,
    source: &str,
    parse_lang: &str,
    lang: &str,
    literals: &mut Vec<Literal>,
) {
    for child in children(node) {
        if matches!(child.kind(), "interpolation" | "template_substitution") {
            for expr in children(child) {
                collect_literals(expr, source, parse_lang, lang, literals);
            }
        } else {
            interpolations(child, source, parse_lang, lang, literals);
        }
    }
}
