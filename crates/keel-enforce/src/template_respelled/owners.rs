//! Reconcile cached owners with head-side callable names from existing scans.
use super::TemplateOccurrence;
use crate::gitdiff::ChangedPath;
use keel_core::template_homes::TemplateHome;
use std::collections::{BTreeMap, BTreeSet};

/// Head-side callable population for every path participating in the diff.
pub(super) struct CurrentOwners {
    changed: BTreeSet<String>,
    callables: BTreeMap<String, BTreeSet<String>>,
}

impl CurrentOwners {
    /// Unchanged paths retain the usual last-map bounded staleness.
    pub(super) fn new(paths: &[ChangedPath]) -> Self {
        Self {
            changed: paths
                .iter()
                .flat_map(|path| {
                    std::iter::once(path.path.clone()).chain(path.base_path().map(String::from))
                })
                .collect(),
            callables: BTreeMap::new(),
        }
    }

    /// Record callable names regardless of current template eligibility.
    pub(super) fn record(&mut self, file: &str, names: BTreeSet<String>) {
        self.callables.insert(file.into(), names);
    }

    /// Keep owners only when their callable survives on the head side.
    pub(super) fn retain(&self, homes: &mut Vec<TemplateHome>) {
        homes.retain(|home| {
            !self.changed.contains(&home.file)
                || self
                    .callables
                    .get(&home.file)
                    .is_some_and(|names| names.contains(&home.name))
        });
    }

    /// Drop removed co-owners and suppress occurrences with no remaining owner.
    pub(super) fn filter(
        &self,
        mut occurrences: Vec<TemplateOccurrence>,
    ) -> Vec<TemplateOccurrence> {
        occurrences.retain_mut(|occurrence| {
            self.retain(&mut occurrence.homes);
            !occurrence.homes.is_empty()
        });
        occurrences
    }
}
