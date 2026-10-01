//! One invocation-wide segment automaton; each literal is searched in one pass.
use super::{ast, eligible_segment, TemplateOccurrence, MIN_SEGMENT_CHARS};
use aho_corasick::AhoCorasick;
use keel_core::template_homes::TemplateHome;
use std::collections::BTreeMap;

/// Compiled segment set shared by every file in one review.
#[derive(Clone)]
pub(super) struct Matcher {
    automaton: AhoCorasick,
    owners: Vec<(String, Vec<TemplateHome>)>,
}
impl Matcher {
    /// Build one automaton from all eligible cached segments.
    pub(super) fn new(homes: &[TemplateHome]) -> Option<Self> {
        let mut owners = BTreeMap::<String, Vec<TemplateHome>>::new();
        for home in homes {
            for segment in &home.segments {
                if eligible_segment(segment, MIN_SEGMENT_CHARS) {
                    owners
                        .entry(segment.clone())
                        .or_default()
                        .push(home.clone());
                }
            }
        }
        if owners.is_empty() {
            return None;
        }
        let owners: Vec<_> = owners.into_iter().collect();
        let automaton = AhoCorasick::new(owners.iter().map(|(s, _)| s)).ok()?;
        Some(Self { automaton, owners })
    }

    /// Share the same automaton while translating cached owners to base paths.
    pub(super) fn base_side(&self, paths: &[crate::gitdiff::ChangedPath]) -> Self {
        let mut base = self.clone();
        let renames: BTreeMap<_, _> = paths
            .iter()
            .filter_map(|path| {
                if let crate::gitdiff::ChangeStatus::Renamed { from } = &path.status {
                    Some((path.path.as_str(), from.as_str()))
                } else {
                    None
                }
            })
            .collect();
        for (_, owners) in &mut base.owners {
            for home in owners {
                if let Some(old) = renames.get(home.file.as_str()) {
                    home.file = (*old).into();
                }
            }
        }
        base
    }

    /// Scan one Git side, excluding both pure and cached owner bodies.
    pub(super) fn occurrences(&self, file: &str, source: &str) -> Vec<TemplateOccurrence> {
        let (current, literals) = ast::scan(file, source);
        self.match_literals(file, current, literals)
    }

    /// Match one side of the structural review's existing parse.
    pub(super) fn parsed_occurrences(
        &self,
        file: &str,
        source: &str,
        parsed: &keel_parsers::resolver::ParseResult,
    ) -> Vec<TemplateOccurrence> {
        let Some(tree) = &parsed.syntax_tree else {
            return vec![];
        };
        let (current, literals) = ast::scan_tree(file, source, tree, &parsed.definitions);
        self.match_literals(file, current, literals)
    }

    fn match_literals(
        &self,
        file: &str,
        current: Vec<ast::HomeSpan>,
        literals: Vec<ast::Literal>,
    ) -> Vec<TemplateOccurrence> {
        let mut matches = Vec::new();
        // Index the few exclusion spans once, rather than scan all homes for
        // every literal/segment pair. Current eligibility is per Git side.
        let exclusions: Vec<Vec<_>> = if current.is_empty() {
            Vec::new()
        } else {
            self.owners
                .iter()
                .map(|(segment, owners)| {
                    current
                        .iter()
                        .filter(|span| {
                            span.eligible && span.home.segments.contains(segment)
                                || owners
                                    .iter()
                                    .any(|home| home.file == file && home.name == span.home.name)
                        })
                        .map(|span| (span.start, span.end))
                        .collect()
                })
                .collect()
        };
        for mut literal in literals {
            let mut found = Vec::new();
            for piece in &literal.pieces {
                for hit in self.automaton.find_overlapping_iter(piece) {
                    found.push(hit.pattern().as_usize());
                }
            }
            found.sort_unstable();
            found.dedup();
            let count = found.len();
            for (position, index) in found.into_iter().enumerate() {
                if exclusions.get(index).is_some_and(|spans| {
                    spans
                        .iter()
                        .any(|&(start, end)| start <= literal.start && literal.end <= end)
                }) {
                    continue;
                }
                let (segment, homes) = &self.owners[index];
                matches.push(TemplateOccurrence {
                    byte_start: literal.start,
                    segment: segment.clone(),
                    homes: homes.clone(),
                    file: file.into(),
                    line: literal.line,
                    literal: if position + 1 == count {
                        std::mem::take(&mut literal.text)
                    } else {
                        literal.text.clone()
                    },
                    literal_parts: if position + 1 == count {
                        std::mem::take(&mut literal.pieces)
                    } else {
                        literal.pieces.clone()
                    },
                });
            }
        }
        matches
    }
}
