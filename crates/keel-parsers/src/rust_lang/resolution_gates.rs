//! Gates that stop a *name-only* Rust match from becoming a wrong edge.
//!
//! Two rules live here, both variants of the same failure: resolving a bare
//! name when nothing at the call site says which definition is meant.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::resolver::{Definition, ParseResult};

/// Macros the Rust prelude and the compiler provide, keyed by their bare name
/// (no `!`). Every one of them is external to the repository being mapped, so
/// an invocation must resolve to nothing at all.
///
/// Without this gate `format!("{x}")` resolved to any same-named *function* in
/// the graph — on one 25k-edge repo a small currency formatter called `format`
/// collected 1,385 phantom callers and became the graph's #1 hotspot,
/// including from files that never import it. Those are `calls` edges, so they
/// feed E001/E004/E005 and mask genuinely dead helpers behind a name
/// collision.
pub const RUST_PRELUDE_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "dbg",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "env",
    "eprint",
    "eprintln",
    "format",
    "include_bytes",
    "include_str",
    "matches",
    "option_env",
    "panic",
    "print",
    "println",
    "todo",
    "unimplemented",
    "unreachable",
    "vec",
    "write",
    "writeln",
];

/// True when `name` (bare, without the trailing `!`) is a prelude or
/// compiler-provided macro — see `RUST_PRELUDE_MACROS`.
pub fn is_rust_prelude_macro(name: &str) -> bool {
    RUST_PRELUDE_MACROS.contains(&name)
}

/// How many distinct files may define a name before a *name-only* cross-file
/// match — one with no import, module path, or receiver type to narrow it —
/// is treated as unresolvable.
///
/// Past two candidates, picking the first cached hit is a coin flip dressed up
/// as a 0.50-confidence edge. An honest absence is strictly better than a
/// wrong `calls` edge, which propagates into E001/E004/E005 and into every
/// caller set an agent reads.
pub const MAX_NAME_ONLY_DEFINITION_FILES: usize = 2;

/// Cached files holding a definition named `name` that satisfies `pred`.
///
/// The shared primitive behind the name-only ambiguity rule: callers compare
/// the result's length against `MAX_NAME_ONLY_DEFINITION_FILES` and emit no
/// edge when the name is spread wider than that.
pub fn files_defining<'a, P>(
    cache: &'a HashMap<PathBuf, ParseResult>,
    name: &str,
    pred: P,
) -> Vec<&'a Path>
where
    P: Fn(&Definition) -> bool,
{
    cache
        .iter()
        .filter(|(_, pr)| pr.definitions.iter().any(|d| d.name == name && pred(d)))
        .map(|(path, _)| path.as_path())
        .collect()
}
