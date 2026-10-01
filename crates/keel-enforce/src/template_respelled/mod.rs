//! Review-only W012 template respelling advisories and precision-study data.
//! These never enter compile findings, violations, gates, or the circuit breaker.
//! Homes are read from the last full map, like fragment-clone measurements.

mod ast;
mod decode;
mod git;
mod homes;
mod literals;
mod matcher;
mod owners;
mod review_scan;

pub(crate) use review_scan::ReviewScan;
#[cfg(test)]
mod tests;

use keel_core::template_homes::TemplateHome;
use serde::{Deserialize, Serialize};

/// Minimum fixed-segment length selected by the independent precision study.
pub const MIN_SEGMENT_CHARS: usize = 24;

/// A decoded literal containing a segment, with all of that segment's owners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateOccurrence {
    /// Literal byte offset, used only to distinguish nodes on the same line.
    #[serde(skip)]
    pub byte_start: usize,
    /// The fixed expression text shared with the home(s).
    pub segment: String,
    /// All owners, including ambiguous ownership.
    pub homes: Vec<TemplateHome>,
    /// Project-relative occurrence path.
    pub file: String,
    /// Start line of the literal, including multiline literals.
    pub line: u32,
    /// Decoded literal text; NUL separates dynamic interpolation holes.
    pub literal: String,
    /// Fixed decoded pieces separated by AST interpolation boundaries. These
    /// disambiguate actual NUL characters from the display's hole separators.
    pub literal_parts: Vec<String>,
}

/// Detector population for precision studies, separate from production advisories.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemplateStudy {
    /// Segment length used for this population.
    pub min_segment_chars: usize,
    /// Pure template functions, including homes with no eligible segments.
    pub template_functions: usize,
    /// Distinct kept segments; shared segments are counted once.
    pub kept_segments: usize,
    /// Mapped homes, retained so threshold studies can recount their segments.
    pub homes: Vec<TemplateHome>,
    /// Baseline-relative literal occurrences, in file/line/segment order.
    pub occurrences: Vec<TemplateOccurrence>,
}

impl TemplateStudy {
    /// Whether this graph has no mapped template population to export.
    pub fn is_empty(&self) -> bool {
        self.template_functions == 0 && self.occurrences.is_empty()
    }
}

/// Whether a trimmed fixed segment has enough characters and expression punctuation.
pub fn eligible_segment(segment: &str, minimum: usize) -> bool {
    segment.chars().count() >= minimum
        && segment
            .chars()
            .any(|c| !c.is_alphanumeric() && c != '_' && !c.is_whitespace())
}

/// Extract every pure non-test home from one supported whole-file source.
pub fn extract_homes(file: &str, source: &str) -> Vec<TemplateHome> {
    let Some(lang) = ast::language(file) else {
        return Vec::new();
    };
    let mut parser = keel_parsers::treesitter::TreeSitterParser::new();
    let Ok(tree) = parser.parse(lang, source.as_bytes()) else {
        return Vec::new();
    };
    extract_homes_from_tree(file, source, &tree, &[])
}

/// Match decoded literals, folding supported concatenations and excluding each
/// owner's current body. A matching literal is counted once per distinct segment;
/// production advisories subsequently deduplicate each literal/home pair.
pub fn occurrences(file: &str, source: &str, homes: &[TemplateHome]) -> Vec<TemplateOccurrence> {
    matcher::Matcher::new(homes).map_or_else(Vec::new, |matcher| matcher.occurrences(file, source))
}

/// Extract homes from map's existing whole-file syntax tree and definitions.
/// No source read, parser construction, literal scan, or second parse is needed.
pub fn extract_homes_from_tree(
    file: &str,
    source: &str,
    tree: &tree_sitter::Tree,
    definitions: &[keel_parsers::resolver::Definition],
) -> Vec<TemplateHome> {
    ast::homes_from_tree(file, source, tree, definitions)
        .into_iter()
        .filter(|span| span.eligible)
        .map(|span| span.home)
        .collect()
}

/// A baseline-new template respelling, advisory only and never a violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateAdvisory {
    /// Review advisory code, always `W012`.
    pub code: String,
    /// Advisory category, always `template_respelled`.
    pub category: String,
    /// Recommendation naming every reported owner and its location.
    pub message: String,
    /// Project-relative literal path.
    pub file: String,
    /// Start line of the matching literal.
    pub line: u32,
    /// Fixed text that matched the reported owners.
    pub segment: String,
    /// Owners not already reported for this literal through another segment.
    pub homes: Vec<TemplateHome>,
}

fn advisories(occurrences: Vec<TemplateOccurrence>) -> Vec<TemplateAdvisory> {
    let mut seen = std::collections::BTreeSet::new();
    occurrences
        .into_iter()
        .filter_map(|occurrence| {
            let homes: Vec<_> = occurrence
                .homes
                .into_iter()
                .filter(|home| {
                    seen.insert((
                        occurrence.file.clone(),
                        occurrence.byte_start,
                        home.file.clone(),
                        home.line,
                        home.name.clone(),
                        home.hash.clone(),
                    ))
                })
                .collect();
            if homes.is_empty() {
                return None;
            }
            let owners = homes
                .iter()
                .map(|home| format!("{} ({}:{})", home.name, home.file, home.line))
                .collect::<Vec<_>>()
                .join(", ");
            Some(TemplateAdvisory {
                code: "W012".into(),
                category: "template_respelled".into(),
                message: format!(
                    "Literal re-spells the fixed text of {owners}: \"{}\" — call it instead.",
                    occurrence.segment
                ),
                file: occurrence.file,
                line: occurrence.line,
                segment: occurrence.segment,
                homes,
            })
        })
        .collect()
}

/// Build review advisories after segment-level Git multiset/move subtraction.
/// Map-derived owners have the same bounded staleness as clone measurements.
pub fn review_advisories(
    store: &dyn keel_core::store::GraphStore,
    root: &std::path::Path,
    commit: &str,
    paths: &[crate::gitdiff::ChangedPath],
    verbose: bool,
) -> Vec<TemplateAdvisory> {
    advisories(export(store, root, commit, paths, verbose).occurrences)
}

/// Build the baseline-relative precision-study population from mapped homes.
/// Unreadable files are skipped on both sides; W012 never fails a review.
pub fn review_export(
    store: &dyn keel_core::store::GraphStore,
    root: &std::path::Path,
    commit: &str,
    paths: &[crate::gitdiff::ChangedPath],
) -> Result<TemplateStudy, String> {
    Ok(export(store, root, commit, paths, false))
}

fn export(
    store: &dyn keel_core::store::GraphStore,
    root: &std::path::Path,
    commit: &str,
    paths: &[crate::gitdiff::ChangedPath],
    verbose: bool,
) -> TemplateStudy {
    let mut homes = mapped_homes(store, paths);
    let mut owners = owners::CurrentOwners::new(paths);
    // No expression can match without a kept segment. Avoid reading unrelated
    // blobs (including non-UTF-8 ones) in reviews with no template population.
    let occurrences = if homes.iter().all(|home| home.segments.is_empty()) {
        Vec::new()
    } else {
        git::introduced(root, commit, paths, &homes, &mut owners, verbose)
    };
    owners.retain(&mut homes);
    let occurrences = owners.filter(occurrences);
    let kept_segments = homes
        .iter()
        .flat_map(|h| &h.segments)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    TemplateStudy {
        min_segment_chars: MIN_SEGMENT_CHARS,
        template_functions: homes.len(),
        kept_segments,
        homes,
        occurrences,
    }
}

fn mapped_homes(
    store: &dyn keel_core::store::GraphStore,
    paths: &[crate::gitdiff::ChangedPath],
) -> Vec<TemplateHome> {
    let mut homes = store.template_homes();
    let deleted: std::collections::BTreeSet<_> = paths
        .iter()
        .filter(|path| {
            matches!(
                path.status,
                crate::gitdiff::ChangeStatus::Deleted
                    | crate::gitdiff::ChangeStatus::RenamedToUnreadable
            )
        })
        .map(|path| path.path.as_str())
        .collect();
    homes.retain(|home| !deleted.contains(home.file.as_str()));
    for home in &mut homes {
        home.segments
            .retain(|s| eligible_segment(s, MIN_SEGMENT_CHARS));
    }
    homes
}

#[cfg(test)]
mod fold_tests;
