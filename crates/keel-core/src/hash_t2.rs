//! Type-2 clone fingerprint (issue #59) — identifier/literal-normalized body hash.
//!
//! [`crate::hash::normalize_body`] is whitespace-only, so two functions that
//! differ only in local/parameter names or literal values get different Type-1
//! fingerprints. Here every non-keyword identifier is renamed positionally
//! (`v0`, `v1`, … in first-appearance order) and literals collapse to `<int>`,
//! `<str>` and `<bool>`, so a copy-paste-then-rename clone fingerprints
//! identically to its original.
//!
//! Deliberately lexical, not AST-based: the only input available where W006
//! runs is `Definition::body_text`, raw source. Comment and string boundaries
//! are delegated to `crate::hash::strip_comments` so the Type-1 and Type-2
//! normalizers can never disagree about what counts as code — including the
//! Rust lifetime-vs-char-literal rule, which is repeated here because the tick
//! survives comment stripping as an ordinary token.
//!
//! **Scope limit vs. a real AST pass:** keywords, builtins and primitive type
//! names are emitted verbatim, so bodies differing by a real type (`i32` vs
//! `String`) stay distinct. A *user-defined* type or class name is
//! indistinguishable from an ordinary identifier at this layer and is renamed
//! like one.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::hash::{self, BodySyntax};

/// Minimum length of the Type-2 normalized token stream worth fingerprinting.
///
/// Higher than [`crate::hash::MIN_INDEXED_BODY_LEN`] and than W006's
/// `MIN_DUPLICATE_BODY_LEN` on purpose: renaming *shrinks* the fingerprinted
/// string (long identifiers become `v3`, string literals become `<str>`), so
/// the Type-1 floor applied to a Type-2 stream would let near-universal
/// trivial shapes (`return v0 . v1 ( v2 ) ;`) collide constantly.
///
/// Calibrated by mapping keel's own tree (issue #59): at 80 the cross-file
/// Type-2-only groups double, and the additions are all shape coincidences —
/// `match self { .. => <str> }` string tables, `&[<str>, <str>]` constant
/// lists, two-line assert loops. At 120 what survives is real duplicated
/// machinery. Raise it before lowering it.
pub const MIN_T2_NORMALIZED_LEN: usize = 120;

/// Placeholder for any numeric literal.
const NUM_TOKEN: &str = "<int>";
/// Placeholder for any string or char literal (contents discarded).
const STR_TOKEN: &str = "<str>";
/// Placeholder for a boolean literal, so `true`/`false` bodies collapse.
const BOOL_TOKEN: &str = "<bool>";

/// Words that stay verbatim in Rust: syntax keywords plus primitive/builtin
/// type and constructor names, whose identity is structural rather than a
/// naming choice.
const RUST_KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "if", "else", "match", "for", "while", "loop", "return", "break",
    "continue", "struct", "enum", "impl", "trait", "pub", "use", "mod", "const", "static", "self",
    "Self", "super", "crate", "as", "where", "in", "ref", "move", "async", "await", "dyn",
    "unsafe", "extern", "type", "yield", "box", "macro", "i8", "i16", "i32", "i64", "i128",
    "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64", "bool", "char", "str",
    "String", "Vec", "Option", "Some", "None", "Result", "Ok", "Err",
];

/// Words that stay verbatim in Python.
const PYTHON_KEYWORDS: &[&str] = &[
    "None", "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
    "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield",
    "self", "int", "str", "float", "bool", "list", "dict", "set", "tuple", "bytes",
];

/// Words that stay verbatim in Go.
#[rustfmt::skip]
const GO_KEYWORDS: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range", "return",
    "select", "struct", "switch", "type", "var", "nil", "iota", "bool", "string", "int", "int8",
    "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr", "byte",
    "rune", "float32", "float64", "error",
];

/// Words that stay verbatim in TypeScript/JavaScript — also the default set
/// for any language keel has no table for.
#[rustfmt::skip]
const JS_TS_KEYWORDS: &[&str] = &[
    "function", "return", "if", "else", "for", "while", "do", "switch", "case", "break",
    "continue", "class", "extends", "implements", "interface", "new", "this", "super", "import",
    "export", "from", "as", "const", "let", "var", "typeof", "instanceof", "in", "of", "try",
    "catch", "finally", "throw", "async", "await", "yield", "void", "delete", "null", "undefined",
    "static", "get", "set", "public", "private", "protected", "readonly", "abstract", "enum",
    "namespace", "declare", "type", "string", "number", "boolean", "any", "unknown", "never",
    "object", "symbol", "bigint",
];

/// The verbatim-word table for `lang`.
///
/// Switches on the raw language name, **not** on [`BodySyntax`]: that enum
/// collapses Go, TypeScript, JavaScript and Svelte into one comment/string
/// family (correct there — same lexical rules), but their keyword and
/// primitive-type vocabularies differ (`func`/`chan` vs `function`/`typeof`).
fn keyword_set(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => RUST_KEYWORDS,
        "python" => PYTHON_KEYWORDS,
        "go" => GO_KEYWORDS,
        _ => JS_TS_KEYWORDS,
    }
}

/// Index just past the string/char literal whose opening delimiter is at `i`.
///
/// Mirrors [`crate::hash::strip_comments`]'s quote rules — including Python
/// triple quotes and backslash escapes — so both normalizers agree on where a
/// literal ends. An unterminated literal consumes to end of input.
fn skip_string(b: &[u8], mut i: usize, syntax: BodySyntax) -> usize {
    let n = b.len();
    let quote = b[i];
    if syntax == BodySyntax::Python && i + 2 < n && b[i + 1] == quote && b[i + 2] == quote {
        i += 3;
        while i < n {
            if b[i] == b'\\' && i + 1 < n {
                i += 2;
                continue;
            }
            if i + 2 < n && b[i] == quote && b[i + 1] == quote && b[i + 2] == quote {
                return i + 3;
            }
            i += 1;
        }
        return n;
    }
    i += 1;
    while i < n {
        let d = b[i];
        if d == b'\\' && i + 1 < n {
            i += 2;
            continue;
        }
        i += 1;
        if d == quote {
            break;
        }
    }
    i
}

/// Index just past the numeric literal starting at `i`.
///
/// Deliberately loose: digits, `_`, hex digits and a trailing suffix run
/// (`1_000u32`, `0x1F`, `10L`) are all one literal. A `.` continues the
/// literal only when a digit follows, so Rust's `1..10` stays a range between
/// two numbers instead of one malformed one.
fn skip_number(b: &[u8], mut i: usize) -> usize {
    let n = b.len();
    if b[i] == b'.' {
        i += 1;
    }
    while i < n {
        if b[i].is_ascii_alphanumeric() || b[i] == b'_' {
            i += 1;
            continue;
        }
        if b[i] == b'.' && i + 1 < n && b[i + 1].is_ascii_digit() {
            i += 1;
            continue;
        }
        break;
    }
    i
}

/// One Type-2 token and the body-relative line it was read from.
///
/// `line` is 0-based and counted over the *comment-stripped* body, so a line
/// holding only a comment carries no token and is invisible to any consumer
/// that counts lines through this type — which is what makes
/// [`crate::fragments`]'s line counts SLOC-like rather than raw-span-like.
/// A multi-line `/* … */` collapses to one line for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionedToken {
    /// The normalized token text — see [`tokenize`].
    ///
    /// Borrowed for every token whose text is fixed (keywords, the three
    /// literal placeholders, ASCII punctuation), owned only for a verbatim
    /// identifier or a renamed `v0`. On keel's own tree that is most of the
    /// ~640k tokens a map produces, so the distinction is the difference
    /// between one allocation per token and one per distinct identifier.
    pub text: Cow<'static, str>,
    /// 0-based line within the comment-stripped body where the token starts.
    pub line: u32,
}

/// What the tokenizer does with a non-keyword identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierMode {
    /// Rename positionally — `v0`, `v1`, … in first-appearance order. The
    /// Type-2 rule: a copy-paste-then-rename fingerprints identically to its
    /// original.
    ///
    /// Whole-body only. The numbering is relative to the *body*, so the same
    /// fragment appearing after a different prefix in two bodies is renamed
    /// differently and the two streams do not match.
    Renamed,
    /// Keep the identifier verbatim.
    ///
    /// What fragment-level matching needs: a window's tokens must mean the
    /// same thing regardless of what came before it in the body.
    Verbatim,
}

/// Tokenize `body` into its Type-2 token stream.
///
/// Comments are dropped, literals collapse to `<int>`/`<str>`/`<bool>`,
/// non-keyword identifiers become `v0`, `v1`, … in first-appearance order, and
/// every other byte is its own single-character token. Whitespace is never
/// emitted, so reformatting is invisible for free.
pub fn tokenize(body: &str, lang: &str) -> Vec<String> {
    tokenize_positioned(body, lang, IdentifierMode::Renamed)
        .into_iter()
        .map(|t| t.text.into_owned())
        .collect()
}

/// The one-character token text for an ASCII punctuation byte, as a `'static`
/// string so operators and separators never allocate.
///
/// `None` for anything else — a non-ASCII byte outside a literal, which falls
/// back to an owned single-char token. Deliberately not exhaustive over ASCII:
/// only the bytes that actually occur as punctuation in the four languages are
/// listed, and the fallback keeps every other byte deterministic.
fn punctuation_token(c: u8) -> Option<&'static str> {
    Some(match c {
        b'(' => "(",
        b')' => ")",
        b'[' => "[",
        b']' => "]",
        b'{' => "{",
        b'}' => "}",
        b'<' => "<",
        b'>' => ">",
        b';' => ";",
        b',' => ",",
        b'.' => ".",
        b':' => ":",
        b'=' => "=",
        b'+' => "+",
        b'-' => "-",
        b'*' => "*",
        b'/' => "/",
        b'%' => "%",
        b'!' => "!",
        b'&' => "&",
        b'|' => "|",
        b'^' => "^",
        b'~' => "~",
        b'?' => "?",
        b'@' => "@",
        b'#' => "#",
        b'$' => "$",
        b'\\' => "\\",
        b'\'' => "'",
        b'"' => "\"",
        b'`' => "`",
        _ => return None,
    })
}

/// [`tokenize`], with each token's body-relative line attached and the
/// identifier rule chosen by the caller.
///
/// The single tokenizer implementation: [`tokenize`] drops the positions and
/// always renames. Fragment-level clone detection needs the positions to
/// translate a matched token window back into a count of source lines, and
/// needs [`IdentifierMode::Verbatim`] because positional renaming is only
/// meaningful over a whole body.
pub fn tokenize_positioned(body: &str, lang: &str, mode: IdentifierMode) -> Vec<PositionedToken> {
    let syntax = hash::body_syntax(lang);
    let stripped = hash::strip_comments(body, syntax);
    let keywords = keyword_set(lang);
    let b = stripped.as_bytes();
    let n = b.len();

    let mut out: Vec<PositionedToken> = Vec::new();
    let mut renames: HashMap<&str, String> = HashMap::new();
    let mut i = 0;
    let mut line = 0u32;
    // Only the whitespace and string-literal branches can cross a newline —
    // identifiers, numbers and single-byte tokens never do — so those two are
    // the only places `line` moves.
    macro_rules! push {
        ($text:expr) => {
            out.push(PositionedToken {
                text: $text.into(),
                line,
            })
        };
    }
    while i < n {
        let c = b[i];
        if c.is_ascii_whitespace() {
            if c == b'\n' {
                line += 1;
            }
            i += 1;
            continue;
        }

        // Rust lifetimes (`'a`) are an unpaired tick, not a literal — same
        // heuristic as `strip_comments`, applied for the same reason: treating
        // it as a string opener swallows the rest of the body.
        if syntax == BodySyntax::Rust && c == b'\'' {
            let is_char_lit = (i + 1 < n && b[i + 1] == b'\\')
                || (i + 2 < n && b[i + 1] != b'\'' && b[i + 2] == b'\'');
            if !is_char_lit {
                push!("'");
                i += 1;
                continue;
            }
        }

        if c == b'"' || c == b'\'' || (syntax == BodySyntax::CFamily && c == b'`') {
            let start = i;
            i = skip_string(b, i, syntax);
            push!(STR_TOKEN);
            line += b[start..i].iter().filter(|&&x| x == b'\n').count() as u32;
            continue;
        }

        if c.is_ascii_digit() || (c == b'.' && i + 1 < n && b[i + 1].is_ascii_digit()) {
            i = skip_number(b, i);
            push!(NUM_TOKEN);
            continue;
        }

        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &stripped[start..i];
            if matches!(word, "true" | "false" | "True" | "False") {
                push!(BOOL_TOKEN);
            } else if let Some(keyword) = keywords.iter().copied().find(|k| *k == word) {
                // The `'static` copy from the table, not the borrowed slice of
                // this body — a keyword token never allocates.
                push!(keyword);
            } else if mode == IdentifierMode::Verbatim {
                push!(word.to_string());
            } else {
                let next = renames.len();
                let token = renames.entry(word).or_insert_with(|| format!("v{next}"));
                push!(token.clone());
            }
            continue;
        }

        // Operators and punctuation, one byte per token: two copies of the same
        // code tokenize identically either way, so no maximal-munch operator
        // table is needed. A non-ASCII byte outside a literal takes the owned
        // fallback — still deterministic, which is the only requirement.
        match punctuation_token(c) {
            Some(text) => push!(text),
            None => push!((c as char).to_string()),
        }
        i += 1;
    }
    out
}

/// Type-2 normalized form of `body`: the renamed token stream joined by spaces.
///
/// Length-gate against [`MIN_T2_NORMALIZED_LEN`] before fingerprinting it.
pub fn normalize_body_t2(body: &str, lang: &str) -> String {
    rename_and_join(
        &tokenize_positioned(body, lang, IdentifierMode::Renamed),
        lang,
    )
}

/// The Type-2 normalized form of an already-tokenized body — the token texts
/// joined by spaces, with non-keyword identifiers renamed positionally.
///
/// Lets a caller that already holds an [`IdentifierMode::Verbatim`] stream
/// derive the renamed form without tokenizing the body a second time. `keel
/// map` needs both: the verbatim stream feeds fragment-level clone detection,
/// the renamed one the Type-2 whole-body fingerprint. The two differ only in
/// how non-keyword identifiers are spelled, and the renaming is positional
/// over the whole stream — which walking these tokens in order reproduces
/// exactly.
///
/// Applying it to a stream that is already renamed is a no-op: `v0` renames to
/// `v0`, since first-appearance order is unchanged.
pub fn rename_and_join(tokens: &[PositionedToken], lang: &str) -> String {
    let keywords = keyword_set(lang);
    let mut renames: HashMap<&str, String> = HashMap::new();
    let mut out = String::new();
    for token in tokens {
        if !out.is_empty() {
            out.push(' ');
        }
        let text: &str = token.text.as_ref();
        // Identifier tokens are the only renameable ones: literals collapse to
        // `<int>`/`<str>`/`<bool>` and punctuation is a single non-alphabetic
        // byte, so neither can start with an ASCII letter or `_`.
        let is_identifier = text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
        if is_identifier && !keywords.contains(&text) {
            let next = renames.len();
            out.push_str(renames.entry(text).or_insert_with(|| format!("v{next}")));
        } else {
            out.push_str(text);
        }
    }
    out
}

/// Type-2 fingerprint of `body` — `base62(xxhash64(normalize_body_t2(body)))`.
///
/// A separate namespace from `hash::compute_body_hash`: same alphabet, never
/// comparable to a Type-1 hash.
pub fn compute_t2_hash(body: &str, lang: &str) -> String {
    hash::hash_string(&normalize_body_t2(body, lang))
}

#[cfg(test)]
#[path = "hash_t2_tests.rs"]
mod tests;
