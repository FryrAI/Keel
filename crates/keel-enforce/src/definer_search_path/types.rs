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

/// How an argument is marked. An `OUT` argument is not part of a routine's
/// identity; `In` stands for every other marker (`IN`, `INOUT`, `VARIADIC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Unmarked,
    In,
    Out,
}

/// The canonical type of one argument, and its mode.
pub(super) fn argument_type(argument: &[Token]) -> (String, Mode) {
    let end = argument
        .iter()
        .position(|token| token.is_word("default") || token.is_punct('='))
        .unwrap_or(argument.len());
    let mut argument = &argument[..end];
    let mut mode = Mode::Unmarked;
    match argument.first() {
        Some(word) if word.is_word("out") => {
            mode = Mode::Out;
            argument = &argument[1..];
        }
        Some(word)
            if ["in", "inout", "variadic"]
                .iter()
                .any(|marker| word.is_word(marker)) =>
        {
            mode = Mode::In;
            argument = &argument[1..];
            if argument.first().is_some_and(|token| token.is_word("out")) {
                argument = &argument[1..]; // `IN OUT`
            }
        }
        _ => {}
    }

    let (mut words, mut suffix, mut depth) = (Vec::<&str>::new(), "", 0usize);
    for token in argument {
        // A modifier (`(10, 2)`) or an array bound is not part of the identity.
        if token.is_punct('(') || token.is_punct('[') {
            if depth == 0 && token.is_punct('[') {
                suffix = "[]"; // PostgreSQL does not keep the number of dimensions
            }
            depth += 1;
        } else if token.is_punct(')') || token.is_punct(']') {
            depth = depth.saturating_sub(1);
        } else if depth > 0 {
            continue;
        } else if token.is_punct('.') {
            words.pop(); // a schema qualifier
        } else if token.is_word("array") {
            suffix = "[]";
        } else if token.kind != Kind::Punct {
            words.push(&token.text);
        }
    }
    // What is left is `[name] type`.
    if words.len() > 1 && !PHRASES.contains(&words.join(" ").as_str()) {
        words.remove(0);
    }
    (canonical(&words.join(" ")).to_string() + suffix, mode)
}

fn canonical(name: &str) -> &str {
    match name {
        "int" | "int4" => "integer",
        "int8" => "bigint",
        "int2" => "smallint",
        "bool" => "boolean",
        "float8" | "float" => "double precision",
        "float4" => "real",
        "varchar" => "character varying",
        "char" | "bpchar" => "character",
        "decimal" | "dec" => "numeric",
        "timestamptz" => "timestamp with time zone",
        "timestamp" => "timestamp without time zone",
        "timetz" => "time with time zone",
        "time" => "time without time zone",
        other => other,
    }
}
