//! The single call-reference resolution ladder shared by `keel map`'s second
//! pass and `keel compile`'s incremental graph sync.
//!
//! Each pipeline used to carry its own ladder. The map's was the full one —
//! tier-2 language resolver → cross-file import → same-file method →
//! same-directory → package import → boundary provider (e.g. BAML). The compile
//! sync re-implemented a weaker subset (same-file local → tier-2 → unique bare
//! name) and, because it *prunes then re-resolves* a file's outgoing call
//! edges, silently deleted every edge the subset could not reproduce (method
//! calls, same-directory, package, and every boundary edge) on each `keel
//! compile`, leaving them gone until the next full map.
//!
//! This module hosts ONE ladder, run by both pipelines through the
//! [`CallIndex`] seam, so a compile's
//! re-resolution reproduces exactly the edges the map would — the prune is
//! lossless. The two pipelines differ only in how they back `CallIndex`: the
//! map with its in-memory indices, the compile sync with graph-backed lookups.

use std::path::Path;

use keel_core::types::EdgeKind;
use keel_parsers::resolver::{Definition, Import, Reference, ReferenceKind};

use super::map_lang_resolve::{resolve_with, ResolverSet};
use super::map_resolve::{
    resolve_cross_file_call, resolve_edge_to_node, resolve_package_import,
    resolve_same_directory_call, resolve_same_file_method, CallIndex,
};

/// Everything about a call site the ladder needs beyond the [`CallIndex`].
pub struct CallSiteCtx<'a> {
    /// The per-language Tier-2 resolvers (some may be absent under `keel compile`).
    pub resolvers: &'a ResolverSet<'a>,
    /// `detect_language` result for the caller file.
    pub language: &'a str,
    /// The caller file, repo-relative.
    pub file_path: &'a str,
    /// Absolute path the resolver cached the caller under (for `resolve_with`).
    pub abs_file: &'a Path,
    /// The caller file's imports.
    pub imports: &'a [Import],
    /// The caller file's definitions (for same-file method resolution).
    pub definitions: &'a [Definition],
}

/// The edge a reference of this kind contributes, paired with the confidence a
/// *same-file* name match carries (cross-file resolution reports its own).
/// `None` for kinds that never produce a reference edge — imports and type
/// refs have their own passes.
///
/// A [`ReferenceKind::Value`] becomes [`EdgeKind::Uses`], never `Calls`: it
/// proves the target is used (so W005 stays quiet) but carries no argument
/// list, so it must never reach broken-caller or arity checking. A
/// [`ReferenceKind::Template`] is the same contract one rung lower — a lexical
/// hit in unparsed markup.
pub fn edge_for_reference(kind: &ReferenceKind) -> Option<(EdgeKind, f64)> {
    match kind {
        ReferenceKind::Call => Some((EdgeKind::Calls, keel_core::confidence::SAME_FILE_CALL)),
        ReferenceKind::Value => Some((EdgeKind::Uses, keel_core::confidence::SAME_FILE_VALUE_REF)),
        ReferenceKind::Template => Some((EdgeKind::Uses, keel_core::confidence::TEMPLATE_LEXICAL)),
        _ => None,
    }
}

/// The resolution tier a *same-file* reference of this kind is recorded under.
///
/// Template references get their own tier so a lexical markup match is
/// distinguishable from a parsed one at every consumer.
pub fn tier_for_reference(kind: &ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Template => TEMPLATE_TIER,
        _ => "tier1",
    }
}

/// Resolution tier recorded for every edge recovered from template markup.
pub const TEMPLATE_TIER: &str = "tier1_template";

/// A resolved call edge target plus the tier and confidence that resolved it.
pub struct ResolvedCall {
    pub target_id: u64,
    pub confidence: f64,
    pub tier: String,
}

/// Resolve one call or value `reference` to a target node, trying each tier in
/// order.
///
/// Returns `None` when no tier resolves it (the caller then leaves the
/// reference unlinked, exactly as the map does). The order and the
/// confidence/tier assignments mirror `keel map`'s second pass verbatim, so a
/// compile-time re-resolution is behaviour-identical to a full map. The ladder
/// resolves a *name* to a definition, so value references (callbacks passed
/// across files) ride the same rungs; only the edge kind the caller stores
/// differs.
pub fn resolve_call_reference(
    idx: &dyn CallIndex,
    ctx: &CallSiteCtx,
    reference: &Reference,
) -> Option<ResolvedCall> {
    let mut confidence = keel_core::confidence::CROSS_FILE_HEURISTIC;
    let mut tier = "tier1".to_string();
    let mut target_id: Option<u64> = None;

    // Tier 2: the language's own `resolve_call_edge` (barrel re-exports, star
    // imports, receiver/interface/trait dispatch). Its reported confidence and
    // tier win verbatim; only its result must map to a known node.
    if let Some(edge) = ctx
        .resolvers
        .for_language(ctx.language)
        .and_then(|r| resolve_with(r, ctx.abs_file, reference))
    {
        let cands = idx.candidates(&edge.target_name);
        if let Some(id) = resolve_edge_to_node(&cands, &edge.target_file) {
            target_id = Some(id);
            confidence = edge.confidence;
            tier = edge.resolution_tier;
        }
    }

    // Cross-file call resolved through the caller's imports.
    if target_id.is_none() {
        target_id = resolve_cross_file_call(&reference.name, ctx.imports, idx);
    }

    // Same-file method/field call (`self.m()`, `obj.m()`): `self`/`this` bind
    // at 0.9, an unfamiliar receiver only on a unique same-file match at 0.7.
    if target_id.is_none() && reference.name.contains('.') {
        if let Some((id, conf)) = resolve_same_file_method(
            &reference.name,
            ctx.file_path,
            ctx.definitions,
            idx.name_to_id(),
        ) {
            target_id = Some(id);
            confidence = conf;
            tier = "tier1_method".to_string();
        }
    }

    // Same-directory call (Go: files in a directory share a package namespace).
    if target_id.is_none() && !reference.name.contains('.') && !reference.name.contains("::") {
        let cands = idx.candidates(&reference.name);
        target_id = resolve_same_directory_call(&cands, ctx.file_path);
    }

    // Cross-package import (monorepo mode).
    if target_id.is_none() && !idx.package_index().is_empty() {
        for imp in ctx.imports {
            if let Some((pkg_tgt, pkg_conf)) =
                resolve_package_import(&reference.name, &imp.source, idx.package_index())
            {
                target_id = Some(pkg_tgt);
                confidence = pkg_conf;
                break;
            }
        }
    }

    // Last resort: a call into a boundary function (e.g. a BAML `.baml`
    // function). The confidence is stored with the matched index entry — the
    // producing provider's own tier, not a hardcoded constant — so a repo with
    // multiple boundary providers records each edge at its provider's tier.
    if target_id.is_none() {
        if let Some((id, conf)) =
            super::map_boundary::resolve_boundary_call(&reference.name, idx.boundary_index())
        {
            target_id = Some(id);
            confidence = conf;
        }
    }

    // A template reference is a whole-word match in unparsed markup. Whichever
    // rung happened to find the name, the *evidence* is still lexical, so the
    // edge is capped at the template confidence and reported under the template
    // tier — an oxc-verified import does not make a markup mention a call site.
    if reference.kind == ReferenceKind::Template {
        confidence = keel_core::confidence::TEMPLATE_LEXICAL;
        tier = TEMPLATE_TIER.to_string();
    }

    target_id.map(|id| ResolvedCall {
        target_id: id,
        confidence,
        tier,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The T1.3 contract: a markup reference is a `uses` edge at template
    /// confidence under the template tier — never a `calls` edge, which would
    /// let a lexical match reach E001/E004/E005 and the fix planner.
    #[test]
    fn template_reference_maps_to_a_uses_edge_at_template_confidence() {
        let (kind, confidence) =
            edge_for_reference(&ReferenceKind::Template).expect("template refs produce an edge");
        assert_eq!(kind, EdgeKind::Uses);
        assert_eq!(confidence, keel_core::confidence::TEMPLATE_LEXICAL);
        assert!(confidence < keel_core::confidence::ERROR_TIER_THRESHOLD);
        assert_eq!(
            tier_for_reference(&ReferenceKind::Template),
            "tier1_template"
        );
    }

    #[test]
    fn call_and_value_references_keep_their_kinds_and_tier() {
        assert_eq!(
            edge_for_reference(&ReferenceKind::Call).map(|(k, _)| k),
            Some(EdgeKind::Calls)
        );
        assert_eq!(
            edge_for_reference(&ReferenceKind::Value).map(|(k, _)| k),
            Some(EdgeKind::Uses)
        );
        assert_eq!(tier_for_reference(&ReferenceKind::Call), "tier1");
        assert_eq!(tier_for_reference(&ReferenceKind::Value), "tier1");
        assert!(edge_for_reference(&ReferenceKind::Import).is_none());
        assert!(edge_for_reference(&ReferenceKind::TypeRef).is_none());
    }
}
