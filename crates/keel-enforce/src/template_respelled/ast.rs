//! Pure template body eligibility and literal traversal.

use super::literals::{children, is_concat, is_literal, literal_segments, text};
use keel_core::template_homes::TemplateHome;
use keel_parsers::resolver::Definition;
use keel_parsers::treesitter::{begins_test_context, detect_language, TreeSitterParser};
use std::path::Path;
use tree_sitter::Node;

/// Current callable body bounds, recalculated even when mapped lines drift.
pub(super) struct HomeSpan {
    pub home: TemplateHome,
    pub start: usize,
    pub end: usize,
    pub eligible: bool,
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

fn walk(node: Node<'_>, visit: &mut impl FnMut(Node<'_>)) {
    visit(node);
    let mut cursor = node.walk();
    for child in node
        .named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
    {
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

fn excluded(file: &str) -> bool {
    // FileClass::Test uses this same predicate; calling it directly also
    // catches test paths nested inside generated clients.
    crate::violations_util::is_test_file(file)
}

/// Extract homes without collecting occurrence literals or parsing again.
pub(super) fn homes_from_tree(
    file: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    definitions: &[Definition],
) -> Vec<HomeSpan> {
    let Some(parse_lang) = language(file).filter(|_| !excluded(file)) else {
        return vec![];
    };
    let lang = if parse_lang == "tsx" {
        "typescript"
    } else {
        parse_lang
    };
    let root = tree.root_node();
    let fmt = lang == "go" && std_fmt(root, source);
    let mut homes = Vec::new();
    let index = super::homes::definition_index(definitions);
    walk_non_test(root, parse_lang, source, false, &mut |node| {
        if let Some(span) = super::homes::span(node, file, source, lang, fmt, &index) {
            homes.push(span);
        }
    });
    homes
}

fn walk_non_test(
    node: Node<'_>,
    lang: &str,
    source: &str,
    in_test: bool,
    visit: &mut impl FnMut(Node<'_>),
) {
    let in_test = in_test || begins_test_context(node, lang, source.as_bytes());
    if in_test {
        return;
    }
    visit(node);
    // Rust/Go literals cannot contain callable syntax. Avoid traversing their
    // content and escape nodes when collecting only callable body spans.
    if matches!(lang, "rust" | "go") && is_literal(node.kind()) {
        return;
    }
    let mut cursor = node.walk();
    for child in node
        .named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
    {
        walk_non_test(child, lang, source, in_test, visit);
    }
}

/// Scan both current callable spans and non-test literals with one parse.
pub(super) fn scan(file: &str, source: &str) -> Option<(Vec<HomeSpan>, Vec<Literal>)> {
    let parse_lang = language(file).filter(|_| !excluded(file))?;
    let mut parser = TreeSitterParser::new();
    let tree = parser.parse(parse_lang, source.as_bytes()).ok()?;
    Some(scan_tree(file, source, &tree, &[]))
}

/// Scan map/review's already-parsed tree, without constructing a parser.
pub(super) fn scan_tree(
    file: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    definitions: &[Definition],
) -> (Vec<HomeSpan>, Vec<Literal>) {
    let Some(parse_lang) = language(file).filter(|_| !excluded(file)) else {
        return (vec![], vec![]);
    };
    let lang = if parse_lang == "tsx" {
        "typescript"
    } else {
        parse_lang
    };
    let fmt = lang == "go" && std_fmt(tree.root_node(), source);
    let mut homes = Vec::new();
    let mut literals = Vec::new();
    let index = super::homes::definition_index(definitions);
    let mut visit = |node: Node<'_>| {
        if let Some(span) = super::homes::span(node, file, source, lang, fmt, &index) {
            homes.push(span);
        }
    };
    collect_literals(
        tree.root_node(),
        source,
        parse_lang,
        lang,
        fmt,
        None,
        &mut visit,
        &mut literals,
    );
    (homes, literals)
}

fn docstring(node: Node<'_>, source: &str) -> bool {
    // Bytes and f-strings are expressions, never Python docstrings.
    let strings = if node.kind() == "concatenated_string" {
        children(node)
    } else {
        vec![node]
    };
    if strings.iter().any(|n| {
        let prefix = text(*n, source)
            .split(['\"', '\''])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        prefix.contains('b') || prefix.contains('f')
    }) {
        return false;
    }
    let mut parent = node.parent();
    while parent.is_some_and(|p| p.kind() == "parenthesized_expression") {
        parent = parent.and_then(|p| p.parent());
    }
    let Some(statement) = parent.filter(|p| p.kind() == "expression_statement") else {
        return false;
    };
    let Some(body) = statement.parent() else {
        return false;
    };
    if children(body)
        .first()
        .is_none_or(|first| first.id() != statement.id())
    {
        return false;
    }
    body.kind() == "module"
        || body.kind() == "block"
            && body
                .parent()
                .is_some_and(|p| matches!(p.kind(), "class_definition" | "function_definition"))
}

#[allow(clippy::too_many_arguments)]
fn collect_literals(
    node: Node<'_>,
    source: &str,
    parse_lang: &str,
    lang: &str,
    fmt: bool,
    skip: Option<usize>,
    visit: &mut impl FnMut(Node<'_>),
    literals: &mut Vec<Literal>,
) {
    if begins_test_context(node, parse_lang, source.as_bytes()) {
        return;
    }
    visit(node);
    if skip == Some(node.id()) {
        interpolations(node, source, parse_lang, lang, fmt, visit, literals);
        return;
    }
    if let Some((literal, pieces)) =
        super::literals::formatted_literal(node, source, lang, fmt, false)
    {
        literals.push(Literal {
            text: pieces.join("\0"),
            pieces,
            line: literal.start_position().row as u32 + 1,
            start: literal.start_byte(),
            end: literal.end_byte(),
        });
        for child in children(node) {
            collect_literals(
                child,
                source,
                parse_lang,
                lang,
                fmt,
                Some(literal.id()),
                visit,
                literals,
            );
        }
        return;
    }
    if is_literal(node.kind()) || is_concat(node, source) {
        if lang == "python" && docstring(node, source) {
            return;
        }
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
        interpolations(node, source, parse_lang, lang, fmt, visit, literals);
        return;
    }
    let mut cursor = node.walk();
    for child in node
        .named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
    {
        collect_literals(child, source, parse_lang, lang, fmt, skip, visit, literals);
    }
}

fn interpolations(
    node: Node<'_>,
    source: &str,
    parse_lang: &str,
    lang: &str,
    fmt: bool,
    visit: &mut impl FnMut(Node<'_>),
    literals: &mut Vec<Literal>,
) {
    if !matches!(lang, "python" | "typescript") {
        return;
    }
    let mut cursor = node.walk();
    for child in node
        .named_children(&mut cursor)
        .filter(|n| !n.kind().contains("comment"))
    {
        if matches!(child.kind(), "interpolation" | "template_substitution") {
            for expr in children(child) {
                collect_literals(expr, source, parse_lang, lang, fmt, None, visit, literals);
            }
        } else {
            interpolations(child, source, parse_lang, lang, fmt, visit, literals);
        }
    }
}
