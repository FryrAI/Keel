//! Wire the per-language `resolve_call_edge` implementations into the map and
//! compile resolution passes.
//!
//! Tier 1 (tree-sitter) leaves cross-file references ambiguous. Before falling
//! back to the path heuristics in [`super::map_resolve`], the passes route each
//! unresolved reference through the language's own `resolve_call_edge` — the
//! sophisticated Tier-2 logic (TypeScript barrel re-exports, Python
//! star-imports, Go receiver/interface dispatch, Rust trait dispatch) that was
//! previously exercised only by unit tests. The resolver reports its own
//! confidence and tier, which the caller stores instead of a hardcoded value.

use std::path::Path;

use keel_parsers::resolver::{CallSite, LanguageResolver, Reference, ResolvedEdge};

/// The per-language resolvers, borrowed for the duration of a resolution pass.
///
/// Fields are `Option` so the same set serves both callers: `keel map` always
/// has all four (each field `Some`), while `keel compile` lazily constructs
/// only the resolvers for the languages it actually touched and leaves the rest
/// `None`.
pub struct ResolverSet<'a> {
    pub ts: Option<&'a dyn LanguageResolver>,
    pub py: Option<&'a dyn LanguageResolver>,
    pub go: Option<&'a dyn LanguageResolver>,
    pub rs: Option<&'a dyn LanguageResolver>,
}

impl<'a> ResolverSet<'a> {
    /// Pick the resolver for a `detect_language` result, or `None` for
    /// languages without a dedicated Tier-2 enhancer (or not constructed).
    pub fn for_language(&self, language: &str) -> Option<&'a dyn LanguageResolver> {
        match language {
            l if keel_parsers::treesitter::is_typescript_family(l) => self.ts,
            "python" => self.py,
            "go" => self.go,
            "rust" => self.rs,
            _ => None,
        }
    }
}

/// Build a [`CallSite`] from a call reference.
///
/// `abs_file` must be the path the resolver cached the caller under (absolute,
/// exactly as passed to `parse_file`) so `resolve_call_edge`'s internal cache
/// lookup hits. The receiver is derived from a qualified name so receiver-aware
/// resolvers (Rust trait dispatch) get a hint; resolvers that key on the full
/// qualified callee (Go `pkg.Func`) read `callee_name` directly.
fn call_site_for(reference: &Reference, abs_file: &Path) -> CallSite {
    let receiver = reference
        .name
        .rsplit_once("::")
        .map(|(r, _)| r.to_string())
        .or_else(|| reference.name.rsplit_once('.').map(|(r, _)| r.to_string()));
    CallSite {
        file_path: abs_file.to_string_lossy().to_string(),
        line: reference.line,
        callee_name: reference.name.clone(),
        receiver,
    }
}

/// Resolve a reference through one resolver's `resolve_call_edge`.
///
/// Returns `None` when the resolver cannot resolve the call — the caller then
/// falls back to the path heuristics.
pub fn resolve_with(
    resolver: &dyn LanguageResolver,
    abs_file: &Path,
    reference: &Reference,
) -> Option<ResolvedEdge> {
    resolver.resolve_call_edge(&call_site_for(reference, abs_file))
}
