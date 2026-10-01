//! Literal-level multiset subtraction. Cross-reference W011's
//! `HomeScanner::introduced_many`: here identity is segment + normalized decoded
//! literal, never a function hash or a source line. Removed copies cancel moves.

use super::{matcher::Matcher, owners::CurrentOwners, TemplateOccurrence};
use crate::gitdiff::{self, ChangeStatus, ChangedPath};
use keel_core::template_homes::TemplateHome;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn identity(occurrence: &TemplateOccurrence) -> (String, Vec<String>) {
    (
        occurrence.segment.clone(),
        occurrence
            .literal_parts
            .iter()
            .map(|piece| piece.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect(),
    )
}

/// Subtract each file's base copies before pooling removals to cancel moves.
pub(super) fn subtract(
    mut head: Vec<TemplateOccurrence>,
    base: Vec<TemplateOccurrence>,
) -> Vec<TemplateOccurrence> {
    head.sort_by(|a, b| {
        (&a.file, a.line, &a.segment, &a.literal).cmp(&(&b.file, b.line, &b.segment, &b.literal))
    });
    if base.is_empty() {
        return head;
    }
    let mut counts = BTreeMap::<_, usize>::new();
    for occurrence in base {
        *counts
            .entry((occurrence.file.clone(), identity(&occurrence)))
            .or_default() += 1;
    }
    let mut additions = Vec::new();
    for occurrence in head {
        let key = (occurrence.file.clone(), identity(&occurrence));
        let count = counts.entry(key).or_default();
        if *count > 0 {
            *count -= 1;
        } else {
            additions.push(occurrence);
        }
    }
    let mut removals = BTreeMap::<_, usize>::new();
    for ((_, key), count) in counts {
        *removals.entry(key).or_default() += count;
    }
    additions.retain(|occurrence| {
        let count = removals.entry(identity(occurrence)).or_default();
        if *count == 0 {
            true
        } else {
            *count -= 1;
            false
        }
    });
    additions
}

/// Read both Git sides and return their literal-level multiset surplus.
pub(super) fn introduced(
    root: &Path,
    commit: &str,
    paths: &[ChangedPath],
    homes: &[TemplateHome],
    owners: &mut CurrentOwners,
    verbose: bool,
) -> Vec<TemplateOccurrence> {
    let mut head = Vec::new();
    let mut base = Vec::new();
    let mut seen_head = BTreeSet::new();
    let mut seen_base = BTreeSet::new();
    let Some(head_matcher) = Matcher::new(homes) else {
        return vec![];
    };
    let base_matcher = head_matcher.base_side(paths);
    for path in paths {
        // Its base literals survive at an unreadable destination, so neither
        // side can contribute to template move cancellation.
        if path.status == ChangeStatus::RenamedToUnreadable {
            continue;
        }
        // Read the entire pair before scanning either side. If one side is
        // unavailable, dropping both avoids inventing baseline-new copies.
        let pair = (|| -> Result<_, String> {
            let old = path
                .base_path()
                .filter(|old| super::ast::language(old).is_some() && seen_base.insert(*old));
            let base = old
                .map(|old| gitdiff::blob_at_checked(root, commit, old))
                .transpose()?
                .flatten();
            let head = if path.status != ChangeStatus::Deleted
                && seen_head.insert(&path.path)
                && super::ast::language(&path.path).is_some()
            {
                let file = root.join(&path.path);
                let meta = std::fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
                if meta.is_file() {
                    Some(std::fs::read_to_string(file).map_err(|e| e.to_string())?)
                } else {
                    None
                }
            } else {
                None
            };
            Ok((old, base, head))
        })();
        match pair {
            Ok((old, before, after)) => {
                if let (Some(old), Some(text)) = (old, before) {
                    let mut occurrences = base_matcher.occurrences(old, &text);
                    // A rename's baseline belongs to its head-side destination.
                    for occurrence in &mut occurrences {
                        occurrence.file.clone_from(&path.path);
                    }
                    base.extend(occurrences);
                }
                if let Some(text) = after {
                    let scanned = head_matcher.scan(&path.path, &text);
                    owners.record(&path.path, scanned.callables);
                    head.extend(scanned.occurrences);
                }
            }
            Err(error) if verbose => eprintln!("keel review: W012 skipping {}: {error}", path.path),
            Err(_) => {}
        }
    }
    subtract(head, base)
}
