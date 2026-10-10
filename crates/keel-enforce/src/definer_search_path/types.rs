//! Argument types as PostgreSQL identifies a routine by them: input arguments
//! only, no names, no defaults, built-in aliases folded to one spelling.

use super::lex::{Kind, Token};

/// Built-in type names of more than one word, so `double precision` is not
/// read as an argument named `double`.
const PHRASES: &[&str] = &[
    "double precision",
    "character varying",
    "bit varying",
    "timestamp with time zone",
    "timestamp without time zone",
    "time with time zone",
    "time without time zone",
];

/// The canonical type of one argument, or `None` for an `OUT` argument, which
/// is not part of a routine's identity.
pub(super) fn input_type(argument: &[Token]) -> Option<String> {
    let end = argument
        .iter()
        .position(|token| token.is_word("default") || token.is_punct('='))
        .unwrap_or(argument.len());
    let mut argument = &argument[..end];
    match argument.first() {
        Some(mode) if mode.is_word("out") => return None,
        Some(mode)
            if ["in", "inout", "variadic"]
                .iter()
                .any(|word| mode.is_word(word)) =>
        {
            argument = &argument[1..];
            if argument.first().is_some_and(|token| token.is_word("out")) {
                argument = &argument[1..]; // `IN OUT`
            }
        }
        _ => {}
    }

    let (mut words, mut suffix, mut depth) = (Vec::<&str>::new(), String::new(), 0usize);
    for token in argument {
        // A modifier (`(10, 2)`) or an array bound is not part of the identity.
        if token.is_punct('(') || token.is_punct('[') {
            if depth == 0 && token.is_punct('[') {
                suffix.push_str("[]");
            }
            depth += 1;
        } else if token.is_punct(')') || token.is_punct(']') {
            depth = depth.saturating_sub(1);
        } else if depth > 0 {
            continue;
        } else if token.is_punct('.') {
            words.pop(); // a schema qualifier
        } else if token.kind != Kind::Punct {
            words.push(&token.text);
        }
    }
    // What is left is `[name] type`.
    if words.len() > 1 && !PHRASES.contains(&words.join(" ").as_str()) {
        words.remove(0);
    }
    Some(canonical(&words.join(" ")).to_string() + &suffix)
}

fn canonical(name: &str) -> &str {
    match name {
        "int" | "int4" => "integer",
        "int8" => "bigint",
        "int2" => "smallint",
        "bool" => "boolean",
        "float8" => "double precision",
        "float4" => "real",
        "varchar" => "character varying",
        "char" | "bpchar" => "character",
        "decimal" => "numeric",
        "timestamptz" => "timestamp with time zone",
        "timestamp" => "timestamp without time zone",
        "timetz" => "time with time zone",
        "time" => "time without time zone",
        other => other,
    }
}
