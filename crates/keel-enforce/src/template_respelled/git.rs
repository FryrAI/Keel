//! Literal-level multiset subtraction. Cross-reference W011's
//! `HomeScanner::introduced_many`: here identity is segment + normalized decoded
//! literal, never a function hash or a source line. Removed copies cancel moves.

use super::{occurrences, TemplateOccurrence};
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

/// Subtract matching base copies, pooling removals across changed files.
pub(super) fn subtract(
    head: Vec<TemplateOccurrence>,
    base: Vec<TemplateOccurrence>,
) -> Vec<TemplateOccurrence> {
    let mut counts = BTreeMap::<_, usize>::new();
    for occurrence in base {
        *counts.entry(identity(&occurrence)).or_default() += 1;
    }
    let mut introduced = Vec::new();
    for occurrence in head {
        let count = counts.entry(identity(&occurrence)).or_default();
        if *count > 0 {
            *count -= 1;
        } else {
            introduced.push(occurrence);
        }
    }
    introduced.sort_by(|a, b| {
        (&a.file, a.line, &a.segment, &a.literal).cmp(&(&b.file, b.line, &b.segment, &b.literal))
    });
    introduced
}

/// Read both Git sides and return their literal-level multiset surplus.
pub(super) fn introduced(
    root: &Path,
    commit: &str,
    paths: &[ChangedPath],
    homes: &[TemplateHome],
) -> Result<Vec<TemplateOccurrence>, String> {
    let mut head = Vec::new();
    let mut base = Vec::new();
    let mut seen_head = BTreeSet::new();
    let mut seen_base = BTreeSet::new();
    let mut base_homes = homes.to_vec();
    for home in &mut base_homes {
        if let Some(old) = paths
            .iter()
            .find(|p| p.path == home.file)
            .and_then(|p| p.base_path())
        {
            home.file = old.into();
        }
    }
    for path in paths {
        if let Some(old) = path.base_path() {
            if seen_base.insert(old) && super::ast::language(old).is_some() {
                if let Some(text) = gitdiff::blob_at_checked(root, commit, old)? {
                    base.extend(occurrences(old, &text, &base_homes));
                }
            }
        }
        if path.status == ChangeStatus::Deleted
            || !seen_head.insert(&path.path)
            || super::ast::language(&path.path).is_none()
        {
            continue;
        }
        let file = root.join(&path.path);
        let meta = std::fs::symlink_metadata(&file).map_err(|e| format!("{}: {e}", path.path))?;
        if !meta.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", path.path))?;
        head.extend(occurrences(&path.path, &text, homes));
    }
    Ok(subtract(head, base))
}
