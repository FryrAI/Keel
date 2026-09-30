//! Language eligibility for ordinary call-resolution rungs.

use std::path::Path;

use keel_parsers::resolver::{Definition, Reference};
use keel_parsers::treesitter::{detect_language, is_typescript_family};

use super::call_binding::{is_bare_call, select_parsed_local_target};
use super::call_resolve::CallSiteCtx;
use super::map_resolve::CallIndex;

/// Whether an ordinary candidate belongs to the caller's callable language.
/// SQL retains its existing behaviour; deliberate boundary surfaces bypass
/// this check at their own rung. Astro's extracted scripts share the TS family.
pub(crate) fn compatible(language: &str, file: &str) -> bool {
    if language == "sql" {
        return true;
    }
    let Some(target) = detect_language(Path::new(file)) else {
        return false;
    };
    let scripts = |lang| is_typescript_family(lang) || lang == "astro";
    target == "sql" || language == target || (scripts(language) && scripts(target))
}

/// Admit a selected ordinary target using the paths already on candidate rows.
/// A deliberate boundary entry is admitted at any rung, retaining its selected
/// confidence. Selection precedes admission: rejecting a base pick cannot
/// rebind to a different file or a later rung. Members still count toward ambiguity.
pub(crate) fn allows_target(idx: &dyn CallIndex, ctx: &CallSiteCtx, name: &str, id: u64) -> bool {
    let allows = |candidates: &[(String, u64)]| {
        candidates
            .iter()
            .any(|(file, candidate)| *candidate == id && compatible(ctx.language, file))
    };
    let bare = name.rsplit(['.', ':']).next().unwrap_or(name);
    idx.boundary_index()
        .get(bare)
        .is_some_and(|&(boundary, _)| boundary == id)
        || allows(&idx.candidates(name))
        || (bare != name && allows(&idx.candidates(bare)))
}

/// Select a fresh parsed local for a bare call, just as map's first pass.
/// Compile uses this only when no stored local exists, including calls inside
/// definitions. Compile handles all-member refusals separately so imported
/// functions retain base compile behaviour.
pub(crate) fn module_local<'a>(
    reference: &Reference,
    file: &str,
    definitions: &'a [Definition],
) -> Option<&'a Definition> {
    if !is_bare_call(reference) {
        return None;
    }
    let locals: Vec<_> = definitions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.name == reference.name)
        .map(|(i, d)| (d, i as u64))
        .collect();
    let selected = locals.last()?.1;
    let id = select_parsed_local_target(reference, selected, file, definitions, &locals)?;
    definitions.get(id as usize)
}

#[cfg(test)]
#[path = "call_language_tests.rs"]
mod tests;
