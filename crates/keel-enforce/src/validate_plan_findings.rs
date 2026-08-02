//! Plan-time findings (`P001` / `P002`) for `keel validate-plan`.
//!
//! These live in a deliberately separate `P` namespace: a plan finding is a
//! claim about code that does not exist yet, so it must never be mixed into the
//! compile violation stream (where it would be invisible on arrival).
//!
//! Both codes are built on the same free-text scan: find `name(...)` call
//! claims in the plan, then check them against the stored graph.
//!
//! - `P001 unknown_symbol` — a bare call target no graph node answers to.
//! - `P002 signature_mismatch` — a call claim whose arity (or return presence)
//!   disagrees with the stored `GraphNode.signature`.
//!
//! **Precision over recall.** A plan is prose; almost every English word is a
//! valid identifier. Every heuristic below exists to make the checker silent
//! rather than wrong, and each one is documented at its use site because a
//! false plan finding costs more trust than a missed one.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use keel_core::types::{GraphNode, NodeKind};

use crate::checkpoint::CallerRef;
use crate::violations_util::{
    countable_arity, is_ident_char, match_paren, parse_signature, split_top_level, strip_receiver,
    ParsedSig,
};

/// Maximum number of plan findings reported for one plan.
const MAX_FINDINGS: usize = 20;

/// A plan-time finding in the `P` namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFinding {
    /// `P001` or `P002`.
    pub code: String,
    /// `WARNING`, or `INFO` once the circuit breaker has downgraded it.
    pub severity: String,
    /// `unknown_symbol` or `signature_mismatch`.
    pub category: String,
    /// The symbol the plan named.
    pub symbol: String,
    /// Human-readable statement of the mismatch.
    pub message: String,
    /// Hash of the stored symbol (empty for `P001` — nothing is stored).
    pub hash: String,
    /// File of the stored symbol (empty for `P001`).
    pub file: String,
    /// Line of the stored symbol (0 for `P001`).
    pub line: u32,
    /// The claim as it appeared in the plan, e.g. `execute(cmd, params)`.
    pub claimed: String,
    /// The stored signature, when there is one.
    pub actual: Option<String>,
    /// What to do about it.
    pub fix_hint: String,
    /// 0.0-1.0, mirroring the compile violation contract.
    pub confidence: f32,
    /// Set once the circuit breaker has seen this finding three times; a
    /// downgraded finding is advisory only and never fails `--strict`.
    #[serde(default)]
    pub downgraded: bool,
}

/// Graph facts the caller (`validate_plan`) already computed, so the finding
/// pass does not redo the resolution work — it borrows the caller's maps as
/// they stand and never touches the store.
pub struct PlanContext<'a> {
    /// Resolved plan tokens -> the winning (most-called) non-module node.
    ///
    /// A *token* is an ASCII identifier of three characters or more. That is
    /// the entire vocabulary this pass can check anything against, and the
    /// reason shorter and non-ASCII claims are silent by construction.
    pub symbol_node: &'a HashMap<String, GraphNode>,
    /// Every plan token the caller looked up -> every node of that name,
    /// modules included. An empty vec means the graph does not know the name;
    /// an absent key means the token was never looked up at all (it was not a
    /// token), which `P001` reads as "cannot tell" and stays silent about.
    pub nodes_by_name: &'a HashMap<String, Vec<GraphNode>>,
    /// Resolved plan tokens -> that winner's callers.
    pub symbol_callers: &'a HashMap<String, Vec<CallerRef>>,
    /// Symbols the plan already declares an intent to change, keyed to the
    /// detected action (`rename`, `change_signature`, ...) and its risk rank.
    pub actions: &'a HashMap<String, (&'static str, u8)>,
}

/// One `name(args)` occurrence found in the plan text.
struct CallClaim {
    name: String,
    /// Preceded by `.` or `::` — a method or path call.
    qualified: bool,
    /// Argument count with any receiver removed; `None` when the argument list
    /// is variadic, defaulted, elided (`...`) or otherwise not countable.
    arity: Option<usize>,
    /// `Some(true)` when the claim spells an explicit `-> T` return; `None`
    /// when the plan says nothing about the return, which is the common case.
    returns: Option<bool>,
    /// `name(args)` as written, for the finding message.
    text: String,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Identifiers that are language keywords, builtins or ubiquitous third-party
/// entry points. They are only consulted for *unresolved* names, so listing a
/// name here can never hide a real repo symbol: if the graph knows the name,
/// `P001` was never in play.
fn call_builtins() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    #[rustfmt::skip]
    const NAMES: &[&str] = &[
        // control flow / declaration keywords across the four languages
        "if", "for", "while", "switch", "match", "return", "catch", "try", "else", "elif",
        "function", "func", "def", "fn", "async", "await", "yield", "lambda", "class", "struct",
        "impl", "enum", "interface", "type", "const", "let", "var", "new", "delete", "throw",
        "assert", "with", "use", "using", "from", "import", "export", "not", "and", "or", "in",
        "is", "defer", "go", "select", "case", "default", "break", "continue",
        // Python builtins
        "print", "len", "str", "int", "float", "bool", "list", "dict", "set", "tuple", "range",
        "enumerate", "zip", "map", "filter", "sorted", "sum", "min", "max", "abs", "open",
        "input", "isinstance", "getattr", "setattr", "hasattr", "super", "self", "this", "repr",
        "hash", "iter", "next", "any", "all", "round", "id",
        // JS / TS globals and test DSL
        "console", "require", "fetch", "Promise", "JSON", "Object", "Array", "String", "Number",
        "Boolean", "Math", "Date", "Error", "setTimeout", "setInterval", "useState", "useEffect",
        "useMemo", "useCallback", "useRef", "describe", "it", "test", "expect", "beforeEach",
        "afterEach", "jest", "vi", "document", "window", "localStorage",
        // Rust prelude / macros
        "Some", "None", "Ok", "Err", "Vec", "Box", "Rc", "Arc", "HashMap", "HashSet", "Option",
        "Result", "format", "println", "eprintln", "vec", "matches", "todo", "unimplemented",
        "panic", "write", "writeln", "assert_eq", "assert_ne", "derive", "cfg", "dbg", "unwrap",
        "clone", "into", "collect",
        // Go builtins
        "make", "append", "cap", "recover", "fmt", "errors", "context",
        // plan boilerplate
        "TODO", "NOTE", "FIXME", "NOTES",
    ];
    SET.get_or_init(|| NAMES.iter().copied().collect())
}

/// Words that mark a line as *proposing* code rather than calling existing
/// code. Any call target named on such a line is excluded from `P001` for the
/// whole plan: "add `computeTotals(a, b)`" is the plan working as intended, not
/// a hallucinated callee.
const CREATION_WORDS: &[&str] = &[
    "add",
    "create",
    "new",
    "introduce",
    "implement",
    "define",
    "extract",
    "write",
    "scaffold",
    "stub",
    "build",
    "generate",
    "rename",
    "replace",
    "wrap",
];

/// Definition keywords: the word right after one of these is being declared,
/// not called (`def foo`, `fn foo`, `function foo`, ...).
const DEFINITION_KEYWORDS: &[&str] = &[
    "def",
    "fn",
    "func",
    "function",
    "class",
    "struct",
    "interface",
    "type",
    "const",
];

/// Scan the plan for `name(...)` call claims.
///
/// Only a *maximal* identifier immediately followed by `(` counts, so prose
/// like "the handler (see below)" never registers. The character before the
/// identifier decides whether the claim is qualified (`.`/`::`) and rejects
/// macro/attribute/interpolation contexts (`!`, `#`, `@`, `$`, `%`, `\`).
fn scan_call_claims(plan: &str) -> Vec<CallClaim> {
    let chars: Vec<char> = plan.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if !is_ident_start(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_ident_char(chars[i]) {
            i += 1;
        }
        if i >= chars.len() || chars[i] != '(' {
            continue;
        }
        let prev = start.checked_sub(1).map(|p| chars[p]);
        if matches!(
            prev,
            Some('!') | Some('#') | Some('@') | Some('$') | Some('%') | Some('\\')
        ) {
            continue;
        }
        let Some(close) = match_paren(&chars, i) else {
            continue;
        };
        let args: String = chars[i + 1..close].iter().collect();
        let name: String = chars[start..i].iter().collect();
        let mut parts = split_top_level(&args);
        strip_receiver(&mut parts);
        let arity = countable_arity(&parts);
        let returns = trailing_return(&chars, close);
        out.push(CallClaim {
            text: format!("{name}({})", args.trim()),
            name,
            qualified: matches!(prev, Some('.') | Some(':')),
            arity,
            returns,
        });
        i = close + 1;
    }
    out
}

/// Whether an explicit `-> T` follows the closing paren. Only `->` counts: a
/// `:` after a call is far more often markdown prose ("`foo(x)`: does Y") than
/// a TypeScript return annotation, and guessing wrong there would fire `P002`
/// on well-formed plans.
fn trailing_return(chars: &[char], close: usize) -> Option<bool> {
    let mut i = close + 1;
    while i < chars.len() && (chars[i] == ' ' || chars[i] == '`') {
        i += 1;
    }
    if i + 2 < chars.len() && chars[i] == '-' && chars[i + 1] == '>' {
        let rest: String = chars[i + 2..chars.len().min(i + 40)].iter().collect();
        if !rest.trim().is_empty() {
            return Some(true);
        }
    }
    None
}

/// Names the plan proposes to create, gathered from creation-verb lines and
/// definition syntax. Excluded from `P001` everywhere in the plan, since a
/// symbol created in step 1 is legitimately called in step 3.
fn proposed_names(plan: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in plan.lines() {
        let words: Vec<&str> = line
            .split(|c: char| !is_ident_char(c))
            .filter(|w| !w.is_empty())
            .collect();
        let creating = words
            .iter()
            .any(|w| CREATION_WORDS.contains(&w.to_lowercase().as_str()));
        if creating {
            for claim in scan_call_claims(line) {
                out.insert(claim.name);
            }
        }
        for pair in words.windows(2) {
            if DEFINITION_KEYWORDS.contains(&pair[0].to_lowercase().as_str()) {
                out.insert(pair[1].to_string());
            }
        }
    }
    out
}

/// Detect `P001`/`P002` findings for a plan.
///
/// Never fails and never panics: the worst case is an empty finding list, which
/// preserves `keel validate-plan`'s documented never-fails contract.
pub fn detect_plan_findings(plan: &str, ctx: &PlanContext<'_>) -> Vec<PlanFinding> {
    // A plan that resolved nothing at all is either about another repo or hit a
    // stale graph; firing P001 on every word of it would be pure noise.
    if ctx.symbol_node.is_empty() {
        return Vec::new();
    }

    let proposed = proposed_names(plan);
    let builtins = call_builtins();
    let claims = scan_call_claims(plan);

    let mut findings: Vec<PlanFinding> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // Symbols whose signature the plan got right at least once: a later
    // partial mention must not contradict a correct one.
    let mut sig_ok: HashSet<String> = HashSet::new();

    // Length floor, applied to BOTH codes. Two-character call shapes are the
    // densest source of prose noise there is — `t("nav.home")`, `cb(err, res)`,
    // `f(x)`, `ok(value)`, and "…out of(pkg)" read as a call to `of`, which in
    // a large graph is a real symbol and fired `P002` at 0.9 confidence. The
    // graph cannot disambiguate a two-letter name from prose, so the checker
    // does not try: short names are silent, which is this module's whole
    // doctrine stated as one line of code.
    for claim in &claims {
        if claim.name.len() < 3 || seen.contains(&claim.name) {
            continue;
        }
        if let Some(node) = ctx.symbol_node.get(&claim.name) {
            if let Some(f) = signature_finding(ctx, claim, node, &mut sig_ok) {
                seen.insert(claim.name.clone());
                findings.push(f);
            }
        } else if let Some(f) = unknown_finding(ctx, claim, &proposed, builtins) {
            seen.insert(claim.name.clone());
            findings.push(f);
        }
        if findings.len() >= MAX_FINDINGS {
            break;
        }
    }

    findings.sort_by(|a, b| a.code.cmp(&b.code).then(a.symbol.cmp(&b.symbol)));
    findings
}

/// `P001` — a bare call target the graph cannot resolve at all.
///
/// The dominant filter is not in this function. A claim whose name is shorter
/// than three characters, or holds a non-ASCII character, is never resolved by
/// `validate_plan` at all and is dropped before reaching here — deliberately,
/// because prose is full of both and neither can be checked. The qualified,
/// builtin, all-caps and proposed-name rules below carry what is left.
fn unknown_finding(
    ctx: &PlanContext<'_>,
    claim: &CallClaim,
    proposed: &HashSet<String>,
    builtins: &HashSet<&'static str>,
) -> Option<PlanFinding> {
    // A dotted or path-qualified call in prose is overwhelmingly a stdlib or
    // third-party method (`.map()`, `serde_json::from_str()`); attributing one
    // to this repo is exactly the false-signal failure keel exists to avoid.
    if claim.qualified
        || builtins.contains(claim.name.as_str())
        || proposed.contains(&claim.name)
        || !claim.name.chars().any(|c| c.is_lowercase())
    {
        return None;
    }
    // Modules count as existing, so consult the unfiltered lookup. An absent
    // key means the name was never looked up — with the length floor already
    // applied above, that leaves the non-ASCII identifiers the caller's
    // tokenizer skips. Read as unresolved, not unknown, and stay silent.
    if !ctx
        .nodes_by_name
        .get(&claim.name)
        .is_some_and(Vec::is_empty)
    {
        return None;
    }
    Some(PlanFinding {
        code: "P001".into(),
        severity: "WARNING".into(),
        category: "unknown_symbol".into(),
        symbol: claim.name.clone(),
        message: format!(
            "Plan calls `{}` but no symbol named `{}` exists in the graph",
            claim.text, claim.name
        ),
        hash: String::new(),
        file: String::new(),
        line: 0,
        claimed: claim.text.clone(),
        actual: None,
        fix_hint: format!(
            "Run `keel search {}` to find the real name. If it is new code, say so in the plan (\"add `{}`\") so keel treats it as proposed rather than missing.",
            claim.name, claim.name
        ),
        confidence: 0.6,
        downgraded: false,
    })
}

/// `P002` — the claimed arity or return presence disagrees with the stored
/// signature.
fn signature_finding(
    ctx: &PlanContext<'_>,
    claim: &CallClaim,
    node: &GraphNode,
    sig_ok: &mut HashSet<String>,
) -> Option<PlanFinding> {
    // The plan already says it is changing this symbol's shape — reporting the
    // shape it is changing *to* as a mismatch would be backwards.
    if matches!(
        ctx.actions.get(claim.name.as_str()).map(|(a, _)| *a),
        Some("rename") | Some("remove") | Some("change_signature") | Some("add_param")
    ) {
        return None;
    }
    // A qualified call only counts when the stored symbol really is a method or
    // associated function; otherwise `foo.map(f)` would be checked against a
    // free function named `map`.
    if claim.qualified && !node.is_associated {
        return None;
    }
    let claimed_arity = claim.arity?;
    if sig_ok.contains(&claim.name) {
        return None;
    }

    // Every same-named candidate has to agree, or the plan's claim cannot be
    // attributed to one of them.
    let candidates: Vec<&GraphNode> = ctx
        .nodes_by_name
        .get(&claim.name)
        .map(|nodes| {
            nodes
                .iter()
                .filter(|n| n.kind != NodeKind::Module)
                .collect()
        })
        .unwrap_or_default();
    if candidates.is_empty() {
        return None;
    }
    let parsed: Vec<ParsedSig> = candidates
        .iter()
        .filter_map(|n| parse_signature(&n.signature))
        .collect();
    if parsed.len() != candidates.len() {
        return None;
    }
    let stored = &parsed[0];
    if parsed
        .iter()
        .any(|p| p.arity != stored.arity || p.has_return != stored.has_return)
    {
        return None;
    }

    let arity_ok = claimed_arity == stored.arity;
    let return_ok = claim.returns.is_none_or(|r| r == stored.has_return);
    if arity_ok && return_ok {
        sig_ok.insert(claim.name.clone());
        return None;
    }

    let callers = ctx.symbol_callers.get(&claim.name).map_or(0, Vec::len);
    let detail = if arity_ok {
        format!(
            "claimed a return type, stored signature has {}",
            if stored.has_return { "one" } else { "none" }
        )
    } else {
        format!(
            "claimed {claimed_arity} argument(s), stored signature takes {}",
            stored.arity
        )
    };
    Some(PlanFinding {
        code: "P002".into(),
        severity: "WARNING".into(),
        category: "signature_mismatch".into(),
        symbol: claim.name.clone(),
        message: format!(
            "Plan signature for `{}` does not match the graph: {detail} (`{}`)",
            claim.name, node.signature
        ),
        hash: node.hash.clone(),
        file: node.file_path.clone(),
        line: node.line_start,
        claimed: claim.text.clone(),
        actual: Some(node.signature.clone()),
        fix_hint: format!(
            "Use the stored signature `{}` ({}:{}); run `keel discover {}` to see its {callers} caller(s) before planning a change.",
            node.signature, node.file_path, node.line_start, node.hash
        ),
        confidence: 0.9,
        downgraded: false,
    })
}

#[cfg(test)]
#[path = "validate_plan_findings_tests.rs"]
mod tests;
