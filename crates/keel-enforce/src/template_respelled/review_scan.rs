//! W012 consumes the structural review's parse, without reading or parsing again.
use super::{
    advisories, git, mapped_homes, matcher::Matcher, owners::CurrentOwners, TemplateAdvisory,
    TemplateOccurrence,
};
use crate::gitdiff::ChangedPath;
use keel_core::store::GraphStore;
use keel_parsers::resolver::ParseResult;
use std::collections::BTreeMap;

/// Invocation-wide matchers and multiset input, collected during structural review.
pub(crate) struct ReviewScan {
    base_matcher: Option<Matcher>,
    head_matcher: Option<Matcher>,
    base: Vec<TemplateOccurrence>,
    head: Vec<TemplateOccurrence>,
    renames: BTreeMap<String, String>,
    owners: CurrentOwners,
    verbose: bool,
}
impl ReviewScan {
    /// Prepare the cached owner population independently for each Git side.
    pub(crate) fn new(store: &dyn GraphStore, paths: &[ChangedPath], verbose: bool) -> Self {
        let homes = mapped_homes(store, paths);
        let head_matcher = Matcher::new(&homes);
        let base_matcher = head_matcher
            .as_ref()
            .map(|matcher| matcher.base_side(paths));
        Self {
            base_matcher,
            head_matcher,
            base: vec![],
            head: vec![],
            renames: paths
                .iter()
                .filter_map(|path| match &path.status {
                    crate::gitdiff::ChangeStatus::Renamed { from } => {
                        Some((from.clone(), path.path.clone()))
                    }
                    _ => None,
                })
                .collect(),
            owners: CurrentOwners::new(paths),
            verbose,
        }
    }

    /// Match a readable base blob using its existing structural parse.
    pub(crate) fn base(&mut self, file: &str, source: &str, parsed: &ParseResult) {
        if let Some(matcher) = &self.base_matcher {
            let mut scanned = matcher.parsed_occurrences(file, source, parsed);
            if let Some(destination) = self.renames.get(file) {
                for occurrence in &mut scanned.occurrences {
                    occurrence.file.clone_from(destination);
                }
            }
            self.base.extend(scanned.occurrences);
        }
    }

    /// Record head callables even when its base is unreadable; match only intact pairs.
    pub(crate) fn head(&mut self, file: &str, source: &str, parsed: &ParseResult, available: bool) {
        if parsed.syntax_tree.is_none() {
            return;
        }
        if let Some(matcher) = &self.head_matcher {
            let scanned = matcher.parsed_occurrences(file, source, parsed);
            self.owners.record(file, scanned.callables);
            if available {
                self.head.extend(scanned.occurrences);
            }
        }
    }

    /// Explain skipped pairs only when W012 has owners and verbosity is requested.
    pub(crate) fn read_error(&self, file: &str, error: &str) {
        if self.verbose && self.head_matcher.is_some() && super::ast::language(file).is_some() {
            eprintln!("keel review: W012 skipping {file}: {error}");
        }
    }

    /// Subtract base copies before deduplicating literal/home advisories.
    pub(crate) fn finish(self) -> Vec<TemplateAdvisory> {
        advisories(self.owners.filter(git::subtract(self.head, self.base)))
    }
}
