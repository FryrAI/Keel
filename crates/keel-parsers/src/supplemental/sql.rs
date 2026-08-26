//! PostgreSQL-aware schema and query extraction.

use std::collections::HashSet;
use std::ops::ControlFlow;
use std::path::Path;

use keel_core::types::NodeKind;
use sqlparser::ast::{Expr as SqlExpr, ObjectName, Statement, Visit, Visitor};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use crate::resolver::{Definition, ParseResult, Reference, ReferenceKind};

use super::{empty_result, line_at, supplemental_definition};

/// Parse PostgreSQL DDL and query references into Keel graph facts.
pub(super) fn parse_sql(path: &Path, content: &str) -> ParseResult {
    let dialect = PostgreSqlDialect {};
    let mut result = empty_result();
    for (start, end) in sql_statement_ranges(content) {
        let segment = &content[start..end];
        let lines = (
            line_at(content, start),
            line_at(content, end.saturating_sub(1)),
        );
        let definition_count = result.definitions.len();
        match Parser::parse_sql(&dialect, segment) {
            Ok(statements) => {
                for statement in statements {
                    add_statement(&statement, path, segment, lines, &mut result);
                }
            }
            Err(_) => {
                if let Some(item) = fallback_definition(path, segment, lines) {
                    result.definitions.push(item);
                }
            }
        }
        let target = result
            .definitions
            .get(definition_count)
            .map(|item| item.name.as_str());
        result
            .references
            .extend(lexical_references(path, segment, lines.0, target));
    }
    deduplicate_references(&mut result.references);
    result
}

fn add_statement(
    statement: &Statement,
    path: &Path,
    body: &str,
    lines: (u32, u32),
    result: &mut ParseResult,
) {
    let target = sql_definition(statement, path, body, lines);
    let target_name = target.as_ref().map(|item| item.name.as_str());
    let mut visitor = SqlReferences::default();
    let _ = statement.visit(&mut visitor);
    if let Statement::Call(function) = statement {
        visitor
            .functions
            .push((object_name(&function.name), function.args.len() as u32));
    }
    for relation in visitor.relations {
        if Some(relation.as_str()) != target_name {
            result.references.push(reference(
                relation,
                path,
                lines.0,
                ReferenceKind::Value,
                None,
            ));
        }
    }
    for (name, arity) in visitor.functions {
        if Some(name.as_str()) != target_name {
            result.references.push(reference(
                name,
                path,
                lines.0,
                ReferenceKind::Call,
                Some(arity),
            ));
        }
    }
    if let Some(item) = target {
        result.definitions.push(item);
    }
}

#[derive(Clone)]
struct LexToken {
    text: String,
    offset: usize,
}

fn lexical_references(
    path: &Path,
    body: &str,
    line_start: u32,
    target: Option<&str>,
) -> Vec<Reference> {
    let tokens = sql_tokens(body, 0);
    let mut relations = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let lower = token.text.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "from" | "join" | "update" | "into" | "references"
        ) {
            if let Some(name) = following_name(&tokens, index + 1) {
                relations.push((name, tokens[index + 1].offset));
            }
        }
        if lower == "table" && index > 0 && tokens[index - 1].text.eq_ignore_ascii_case("alter") {
            if let Some(name) = following_name(&tokens, index + 1) {
                relations.push((name, tokens[index + 1].offset));
            }
        }
    }
    if starts_create_kind(&tokens, &["index", "policy", "trigger"]) {
        if let Some(on) = tokens
            .iter()
            .position(|token| token.text.eq_ignore_ascii_case("on"))
        {
            if let Some(name) = following_name(&tokens, on + 1) {
                relations.push((name, tokens[on + 1].offset));
            }
        }
    }

    let relation_names: HashSet<String> = relations.iter().map(|(name, _)| name.clone()).collect();
    let mut references = Vec::new();
    for (name, offset) in relations {
        if Some(name.as_str()) != target {
            references.push(reference(
                name,
                path,
                line_start + line_at(body, offset) - 1,
                ReferenceKind::Value,
                None,
            ));
        }
    }
    for window in tokens.windows(2) {
        let name = clean_name(&window[0].text);
        if window[1].text == "("
            && !is_sql_keyword(&name)
            && !relation_names.contains(&name)
            && Some(name.as_str()) != target
        {
            references.push(reference(
                name,
                path,
                line_start + line_at(body, window[0].offset) - 1,
                ReferenceKind::Call,
                None,
            ));
        }
    }
    references
}

fn following_name(tokens: &[LexToken], index: usize) -> Option<String> {
    let token = tokens.get(index)?;
    (token.text != "(").then(|| clean_name(&token.text))
}

fn starts_create_kind(tokens: &[LexToken], kinds: &[&str]) -> bool {
    let Some(create) = tokens
        .iter()
        .position(|token| token.text.eq_ignore_ascii_case("create"))
    else {
        return false;
    };
    tokens[create + 1..].iter().take(4).any(|token| {
        kinds
            .iter()
            .any(|kind| token.text.eq_ignore_ascii_case(kind))
    })
}

fn is_sql_keyword(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "as" | "begin"
            | "case"
            | "check"
            | "create"
            | "default"
            | "exists"
            | "function"
            | "if"
            | "in"
            | "index"
            | "language"
            | "not"
            | "on"
            | "or"
            | "policy"
            | "primary"
            | "procedure"
            | "references"
            | "return"
            | "returns"
            | "select"
            | "table"
            | "then"
            | "trigger"
            | "unique"
            | "values"
            | "when"
            | "where"
    )
}

fn clean_name(raw: &str) -> String {
    raw.trim_matches('"')
        .rsplit('.')
        .next()
        .unwrap_or(raw)
        .to_string()
}

fn deduplicate_references(references: &mut Vec<Reference>) {
    let mut seen = HashSet::new();
    references.retain(|item| {
        let kind = match item.kind {
            ReferenceKind::Call => 0,
            ReferenceKind::Value => 1,
            _ => 2,
        };
        seen.insert((item.name.clone(), item.line, kind))
    });
}

fn sql_tokens(content: &str, base: usize) -> Vec<LexToken> {
    let bytes = content.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"--") {
            index = bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |end| index + end + 1);
        } else if bytes[index..].starts_with(b"/*") {
            index = find_bytes(bytes, index + 2, b"*/").map_or(bytes.len(), |end| end + 2);
        } else if bytes[index] == b'\'' {
            index = quoted_end(bytes, index, b'\'');
        } else if bytes[index] == b'"' {
            let end = quoted_end(bytes, index, b'"');
            tokens.push(LexToken {
                text: content[index + 1..end.saturating_sub(1)].to_string(),
                offset: base + index,
            });
            index = end;
        } else if bytes[index] == b'$' {
            if let Some(tag_end) = dollar_tag_end(bytes, index) {
                let tag = &bytes[index..tag_end];
                if let Some(close) = find_bytes(bytes, tag_end, tag) {
                    tokens.extend(sql_tokens(&content[tag_end..close], base + tag_end));
                    index = close + tag.len();
                } else {
                    index = tag_end;
                }
            } else {
                index += 1;
            }
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || matches!(bytes[index], b'_' | b'.' | b'$'))
            {
                index += 1;
            }
            tokens.push(LexToken {
                text: content[start..index].to_string(),
                offset: base + start,
            });
        } else {
            if bytes[index] == b'(' {
                tokens.push(LexToken {
                    text: "(".into(),
                    offset: base + index,
                });
            }
            index += 1;
        }
    }
    tokens
}

fn quoted_end(bytes: &[u8], mut index: usize, quote: u8) -> usize {
    index += 1;
    while index < bytes.len() {
        if bytes[index] == quote {
            if bytes.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn find_bytes(bytes: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    bytes[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

fn sql_definition(
    statement: &Statement,
    path: &Path,
    body: &str,
    lines: (u32, u32),
) -> Option<Definition> {
    let (name, kind, signature) = match statement {
        Statement::CreateTable { name, .. } => class_definition("TABLE", name),
        Statement::CreateView { name, .. } => class_definition("VIEW", name),
        Statement::CreateType { name, .. } => class_definition("TYPE", name),
        Statement::CreateFunction {
            name,
            args,
            return_type,
            ..
        } => {
            let name = object_name(name);
            let args = display_items(args.as_deref().unwrap_or_default());
            let returns = return_type
                .as_ref()
                .map(|kind| format!(" -> {kind}"))
                .unwrap_or_default();
            (
                name.clone(),
                NodeKind::Function,
                format!("{name}({args}){returns}"),
            )
        }
        Statement::CreateProcedure { name, params, .. } => {
            callable_definition(name, display_items(params.as_deref().unwrap_or_default()))
        }
        Statement::CreateMacro { name, args, .. } => {
            callable_definition(name, display_items(args.as_deref().unwrap_or_default()))
        }
        _ => return None,
    };
    Some(schema_definition(name, kind, signature, path, lines, body))
}

fn class_definition(kind: &str, name: &ObjectName) -> (String, NodeKind, String) {
    let name = object_name(name);
    (
        name.clone(),
        NodeKind::Class,
        format!("CREATE {kind} {name}"),
    )
}

fn callable_definition(name: &ObjectName, parameters: String) -> (String, NodeKind, String) {
    let name = object_name(name);
    (
        name.clone(),
        NodeKind::Function,
        format!("{name}({parameters})"),
    )
}

fn display_items<T: ToString>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Default)]
struct SqlReferences {
    relations: Vec<String>,
    functions: Vec<(String, u32)>,
}

impl Visitor for SqlReferences {
    type Break = ();

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        self.relations.push(object_name(relation));
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expression: &SqlExpr) -> ControlFlow<Self::Break> {
        if let SqlExpr::Function(function) = expression {
            self.functions
                .push((object_name(&function.name), function.args.len() as u32));
        }
        ControlFlow::Continue(())
    }
}

fn object_name(name: &ObjectName) -> String {
    name.0
        .last()
        .map(|identifier| identifier.value.clone())
        .unwrap_or_default()
}

fn reference(
    name: String,
    path: &Path,
    line: u32,
    kind: ReferenceKind,
    call_arity: Option<u32>,
) -> Reference {
    Reference {
        name,
        file_path: path.to_string_lossy().to_string(),
        line,
        kind,
        resolved_to: None,
        call_arity,
    }
}

fn fallback_definition(path: &Path, body: &str, lines: (u32, u32)) -> Option<Definition> {
    let tokens: Vec<&str> = body
        .split(|character: char| character.is_whitespace() || matches!(character, '(' | ';'))
        .filter(|token| !token.is_empty())
        .collect();
    let mut index = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case("create"))?
        + 1;
    while optional_create_word(tokens.get(index).copied()) {
        index += 1;
    }
    let kind = *tokens.get(index)?;
    index += 1;
    while ["if", "not", "exists"].iter().any(|word| {
        tokens
            .get(index)
            .is_some_and(|token| token.eq_ignore_ascii_case(word))
    }) {
        index += 1;
    }
    let name = tokens
        .get(index)?
        .trim_matches(['"', '`'])
        .rsplit('.')
        .next()?
        .to_string();
    let node_kind = match kind.to_ascii_lowercase().as_str() {
        "table" | "view" | "type" => NodeKind::Class,
        "function" | "procedure" | "macro" | "trigger" => NodeKind::Function,
        _ => return None,
    };
    Some(schema_definition(
        name.clone(),
        node_kind,
        format!("CREATE {} {name}", kind.to_ascii_uppercase()),
        path,
        lines,
        body,
    ))
}

fn schema_definition(
    name: String,
    kind: NodeKind,
    signature: String,
    path: &Path,
    lines: (u32, u32),
    body: &str,
) -> Definition {
    let callable = kind == NodeKind::Function;
    let mut item = supplemental_definition(name, kind, signature, path, lines, body.into());
    if callable {
        // Migration routines are database/runtime contract entries: callers
        // often live in triggers, grants, PostgREST, or newer migrations rather
        // than a statically visible source call. Replacement declarations with
        // the same name are also normal across a migration history.
        item.is_auto_invoked = true;
        item.is_associated = true;
        item.in_trait_context = true;
    }
    item
}

fn optional_create_word(token: Option<&str>) -> bool {
    token.is_some_and(|token| {
        ["or", "replace", "temporary", "temp"]
            .iter()
            .any(|word| token.eq_ignore_ascii_case(word))
    })
}

/// Split SQL without treating semicolons in quotes or dollar bodies as boundaries.
pub(super) fn sql_statement_ranges(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut ranges = Vec::new();
    let (mut start, mut index) = (0usize, 0usize);
    let (mut single, mut double, mut line_comment, mut block_comment) =
        (false, false, false, false);
    let mut dollar: Option<Vec<u8>> = None;
    while index < bytes.len() {
        if line_comment {
            line_comment = bytes[index] != b'\n';
            index += 1;
            continue;
        }
        if block_comment {
            if bytes[index..].starts_with(b"*/") {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(tag) = dollar.as_ref() {
            if bytes[index..].starts_with(tag) {
                index += tag.len();
                dollar = None;
            } else {
                index += 1;
            }
            continue;
        }
        if single {
            if bytes[index] == b'\'' {
                if bytes.get(index + 1) == Some(&b'\'') {
                    index += 2;
                    continue;
                }
                single = false;
            }
            index += 1;
            continue;
        }
        if double {
            double = bytes[index] != b'"';
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"--") {
            line_comment = true;
            index += 2;
        } else if bytes[index..].starts_with(b"/*") {
            block_comment = true;
            index += 2;
        } else if bytes[index] == b'\'' {
            single = true;
            index += 1;
        } else if bytes[index] == b'"' {
            double = true;
            index += 1;
        } else if bytes[index] == b'$' {
            if let Some(end) = dollar_tag_end(bytes, index) {
                dollar = Some(bytes[index..end].to_vec());
                index = end;
            } else {
                index += 1;
            }
        } else if bytes[index] == b';' {
            if !content[start..index].trim().is_empty() {
                ranges.push((start, index + 1));
            }
            start = index + 1;
            index += 1;
        } else {
            index += 1;
        }
    }
    if !content[start..].trim().is_empty() {
        ranges.push((start, content.len()));
    }
    ranges
}

fn dollar_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let relative = bytes[start + 1..]
        .iter()
        .position(|byte| *byte == b'$' || !(byte.is_ascii_alphanumeric() || *byte == b'_'))?;
    let end = start + relative + 2;
    (bytes.get(end - 1) == Some(&b'$')).then_some(end)
}
