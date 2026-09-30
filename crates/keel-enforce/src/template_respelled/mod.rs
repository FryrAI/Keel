//! Phase-1 template respelling detector and study export. No warning code,
//! compile finding, gate, or human/LLM recommendation is enabled by this module.
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

/// Initial study threshold; the study also filters this population at 24.
pub const MIN_SEGMENT_CHARS: usize = 16;

/// A decoded literal containing a segment, with all of that segment's owners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateOccurrence {
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

/// Unjudged study payload, separate from review's violations and gates.
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
/// owner's current body. A matching literal is counted once per distinct segment.
pub fn occurrences(file: &str, source: &str, homes: &[TemplateHome]) -> Vec<TemplateOccurrence> {
    let (current, literals) = ast::scan(file, source);
    let mut owners: BTreeMap<&str, Vec<TemplateHome>> = BTreeMap::new();
    for home in homes {
        for segment in &home.segments {
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

/// Build the unjudged, baseline-relative JSON study payload from mapped homes.
/// Errors reading base blobs are surfaced rather than silently inventing additions.
pub fn review_export(
    store: &dyn keel_core::store::GraphStore,
    root: &std::path::Path,
    commit: &str,
    paths: &[crate::gitdiff::ChangedPath],
) -> Result<TemplateStudy, String> {
    let homes = store.template_homes();
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
