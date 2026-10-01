//! Reconcile cached owners with head-side callable names from existing scans.
use super::TemplateOccurrence;
use crate::gitdiff::{ChangeStatus, ChangedPath};
use keel_core::template_homes::TemplateHome;
use std::collections::{BTreeMap, BTreeSet};

/// Head-side callable population for every path participating in the diff.
pub(super) struct CurrentOwners {
    changed: BTreeSet<String>,
    renamed: BTreeMap<String, String>,
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
            renamed: paths
                .iter()
                .filter_map(|path| match &path.status {
                    ChangeStatus::Renamed { from } => Some((from.clone(), path.path.clone())),
                    _ => None,
                })
                .collect(),
            callables: BTreeMap::new(),
        }
    }

    /// Record callable names regardless of current template eligibility.
    pub(super) fn record(&mut self, file: &str, names: BTreeSet<String>) {
        self.callables.insert(file.into(), names);
    }

    /// Prune only confirmed absence; an unavailable head scan retains cached owners.
    pub(super) fn retain(&self, homes: &mut Vec<TemplateHome>) {
        homes.retain(|home| {
            !self.changed.contains(&home.file)
                || self
                    .callables
                    .get(self.renamed.get(&home.file).unwrap_or(&home.file))
                    .is_none_or(|names| names.contains(&home.name))
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
