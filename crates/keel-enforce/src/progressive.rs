//! Progressive adoption: E002/E003 debt is pre-existing when the Git base
//! holds the same name and plain definition hash. Each base definition can
//! vouch for only one working-tree definition, in parse order.
//!
//! Working-tree compiles use HEAD; `compile --since` uses its supplied ref.
//! An indexed rename can supply the old path's blob. Uncommitted new or edited
//! code stays ERROR until committed or reverted to the base text.
//!
//! Without a configured base, a Git checkout, or a resolvable commit, the
//! graph baseline applies silently. KNOWN LIMITATION of that fallback:
//! a full `keel map` clears history and can forgive edited-but-unfixed debt.
//! Once a commit resolves, missing or unreadable source never downgrades.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use keel_core::types::GraphNode;
use keel_parsers::resolver::FileIndex;

use crate::gitdiff::{self, ChangeStatus};
use crate::parse_util::BlobParser;
use crate::types::Violation;

const PROGRESSIVE_CODES: &[&str] = &["E002", "E003"];

/// Lazy Git baseline scoped to one compile, independent of graph history.
pub struct GitBase {
    root: PathBuf,
    rev: String,
    commit: Option<Result<String, String>>,
    renames: Option<Result<HashMap<String, String>, String>>,
    parser: BlobParser,
}

impl GitBase {
    /// Prepare a baseline without running Git or constructing any grammar.
    pub fn new(root: &Path, rev: &str) -> Self {
        Self {
            root: root.to_path_buf(),
            rev: rev.into(),
            commit: None,
            renames: None,
            parser: BlobParser::new(),
        }
    }

    fn definitions(&mut self, path: &str) -> Option<HashMap<(String, String), usize>> {
        if !self.root.join(".git").exists() {
            return None;
        }
        let commit = self
            .commit
            .get_or_insert_with(|| gitdiff::resolve_commit(&self.root, &self.rev))
            .as_ref()
            .ok()?
            .clone();
        match self.read_definitions(path, &commit) {
            Ok(definitions) => Some(definitions),
            Err(reason) => {
                eprintln!(
                    "keel: cannot read {path} at Git base {} ({reason}); its E002/E003 stay ERROR",
                    commit.chars().take(12).collect::<String>()
                );
                Some(HashMap::new())
            }
        }
    }

    fn read_definitions(
        &mut self,
        path: &str,
        commit: &str,
    ) -> Result<HashMap<(String, String), usize>, String> {
        let mut base_path = path.to_string();
        let mut text = gitdiff::source_at(&self.root, commit, path)?;
        if text.is_none() {
            let renames = self.renames.get_or_insert_with(|| {
                gitdiff::changed_paths_staged(&self.root, commit).map(|changes| {
                    changes
                        .into_iter()
                        .filter_map(|change| match change.status {
                            ChangeStatus::Renamed { from } => Some((change.path, from)),
                            _ => None,
                        })
                        .collect()
                })
            });
            if let Some(from) = renames.as_ref().map_err(Clone::clone)?.get(path) {
                base_path = from.clone();
                text = gitdiff::source_at(&self.root, commit, from)?;
            }
        }
        let mut counts = HashMap::new();
        if let Some(mut text) = text {
            // Signature whitespace participates in hashes; Git may store LF
            // while the checkout uses CRLF, so parse both in checkout form.
            if !text.contains('\r')
                && std::fs::read_to_string(self.root.join(path))
                    .is_ok_and(|working| working.contains("\r\n"))
            {
                text = text.replace('\n', "\r\n");
            }
            if let Some(file) = self.parser.parse(&base_path, &text) {
                for def in file.definitions {
                    let hash = def.hash();
                    *counts.entry((def.name, hash)).or_default() += 1;
                }
            }
        }
        Ok(counts)
    }
}

/// Downgrade unchanged E002/E003 debt, using Git or the no-Git graph rule.
pub fn apply_progressive_adoption(
    violations: Vec<Violation>,
    file: &FileIndex,
    existing_nodes: &[GraphNode],
    base: Option<&mut GitBase>,
) -> Vec<Violation> {
    if !violations.iter().any(eligible) {
        return violations;
    }
    let mut counts = base.and_then(|base| base.definitions(&file.file_path));
    let untouched: Vec<bool> = file
        .definitions
        .iter()
        .map(|def| {
            if let Some(counts) = counts.as_mut() {
                let count = counts.entry((def.name.clone(), def.hash())).or_default();
                if *count == 0 {
                    return false;
                }
                *count -= 1;
                true
            } else {
                crate::violations_util::bind_to_node(def, file, existing_nodes).is_some_and(
                    |node| {
                        node.previous_hashes.is_empty()
                            && crate::violations_util::node_hash_matches(node, def, &file.file_path)
                    },
                )
            }
        })
        .collect();
    violations
        .into_iter()
        .map(|mut v| {
            if eligible(&v)
                && file
                    .definitions
                    .iter()
                    .position(|def| def.line_start == v.line)
                    .is_some_and(|index| untouched[index])
            {
                v.severity = "WARNING".to_string();
                v.fix_hint = Some(format!(
                    "{} (pre-existing — escalates to ERROR when this function is edited)",
                    v.fix_hint.unwrap_or_default()
                ));
            }
            v
        })
        .collect()
}

fn eligible(v: &Violation) -> bool {
    v.severity == "ERROR" && PROGRESSIVE_CODES.contains(&v.code.as_str())
}

#[cfg(test)]
#[path = "progressive_tests.rs"]
mod tests;
