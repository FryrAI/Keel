//! Literal-level multiset subtraction. Cross-reference W011's
//! `HomeScanner::introduced_many`: here identity is segment + normalized decoded
//! literal, never a function hash or a source line. Removed copies cancel moves.

use super::{matcher::Matcher, TemplateOccurrence};
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
    let mut introduced = if base.is_empty() {
        head
    } else {
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
        introduced
    };
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
                    base.extend(base_matcher.occurrences(old, &text));
                }
                if let Some(text) = after {
                    head.extend(head_matcher.occurrences(&path.path, &text));
                }
            }
            Err(error) if verbose => eprintln!("keel review: W012 skipping {}: {error}", path.path),
            Err(_) => {}
        }
    }
    subtract(head, base)
}
