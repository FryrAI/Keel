//! Review-only W012 template respelling advisories and precision-study data.
//! These never enter compile findings, violations, gates, or the circuit breaker.
//! Homes are read from the last full map, like fragment-clone measurements.

mod ast;
mod decode;
mod git;
mod literals;
#[cfg(test)]
mod tests;

use keel_core::template_homes::TemplateHome;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    ast::scan(file, source)
        .0
        .into_iter()
        .map(|span| span.home)
        .collect()
}

/// Match decoded literals, folding supported concatenations and excluding each
/// owner's current body. A matching literal is counted once per distinct segment;
/// production advisories subsequently deduplicate each literal/home pair.
pub fn occurrences(file: &str, source: &str, homes: &[TemplateHome]) -> Vec<TemplateOccurrence> {
    let (current, literals) = ast::scan(file, source);
    let mut owners: BTreeMap<&str, Vec<TemplateHome>> = BTreeMap::new();
    for home in homes {
        for segment in &home.segments {
            // Older map caches may still contain the study's shorter segments.
            if !eligible_segment(segment, MIN_SEGMENT_CHARS) {
                continue;
            }
            owners.entry(segment).or_default().push(home.clone());
        }
    }
    let mut matches = Vec::new();
    for literal in literals {
        for (segment, homes) in &owners {
            if !literal.pieces.iter().any(|piece| piece.contains(segment)) {
                continue;
            }
            let own_body = current.iter().any(|span| {
                span.start <= literal.start
                    && literal.end <= span.end
                    && span.home.segments.iter().any(|s| s == segment)
                    && homes
                        .iter()
                        .any(|h| h.file == file && h.name == span.home.name)
            });
            if own_body {
                continue;
            }
            matches.push(TemplateOccurrence {
                byte_start: literal.start,
                segment: (*segment).into(),
                homes: homes.clone(),
                file: file.into(),
                line: literal.line,
                literal: literal.text.clone(),
                literal_parts: literal.pieces.clone(),
            });
        }
    }
    matches
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
) -> Result<Vec<TemplateAdvisory>, String> {
    Ok(advisories(
        review_export(store, root, commit, paths)?.occurrences,
    ))
}

/// Build the baseline-relative precision-study population from mapped homes.
/// Errors reading base blobs are surfaced rather than silently inventing additions.
pub fn review_export(
    store: &dyn keel_core::store::GraphStore,
    root: &std::path::Path,
    commit: &str,
    paths: &[crate::gitdiff::ChangedPath],
) -> Result<TemplateStudy, String> {
    let mut homes = store.template_homes();
    for home in &mut homes {
        home.segments
            .retain(|s| eligible_segment(s, MIN_SEGMENT_CHARS));
    }
    let kept_segments = homes
        .iter()
        .flat_map(|h| &h.segments)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // No expression can match without a kept segment. Avoid reading unrelated
    // blobs (including non-UTF-8 ones) in reviews with no template population.
    let occurrences = if kept_segments == 0 {
        Vec::new()
    } else {
        git::introduced(root, commit, paths, &homes)?
    };
    Ok(TemplateStudy {
        min_segment_chars: MIN_SEGMENT_CHARS,
        template_functions: homes.len(),
        kept_segments,
        homes,
        occurrences,
    })
}
