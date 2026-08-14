//! Baseline-relative code-surface growth for `keel review`.
//!
//! These are facts, not violations. New files and symbols are often exactly
//! what a feature needs; the ledger makes unusually creation-heavy changes
//! visible without pretending Keel can infer how many "features" a PR holds.

use std::collections::{HashMap, HashSet};

use keel_core::types::NodeKind;
use keel_parsers::resolver::{Definition, FileIndex, ReferenceKind};
use serde::{Deserialize, Serialize};

use crate::file_class::FileClass;
use crate::gitdiff::{ChangeStatus, ChangedPath};

use super::diff::DiffScan;
use super::ChangeKind;

/// Additive production-surface changes introduced by one review diff.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SprawlLedger {
    pub source_files_added: usize,
    pub functions_added: usize,
    pub public_symbols_added: usize,
    pub existing_functions_modified: usize,
    pub single_consumer_helpers: usize,
    pub single_consumer_modules: usize,
    /// `functions_added / (functions_added + existing_functions_modified)`.
    pub creation_bias: f64,
}

impl SprawlLedger {
    /// Whether the diff grew no tracked production surface.
    pub fn is_empty(&self) -> bool {
        self.source_files_added == 0
            && self.functions_added == 0
            && self.public_symbols_added == 0
            && self.single_consumer_helpers == 0
            && self.single_consumer_modules == 0
    }
}

/// Measure one diff's additive code surface and consumer shape.
pub fn measure(paths: &[ChangedPath], scan: &DiffScan) -> SprawlLedger {
    let added_files: HashSet<&str> = paths
        .iter()
        .filter(|p| p.status == ChangeStatus::Added)
        .filter(|p| FileClass::classify(&p.path).grades_size_and_naming())
        .map(|p| p.path.as_str())
        .collect();
    let functions_added = scan
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Added && c.symbol_kind == NodeKind::Function)
        .count();
    let public_symbols_added = scan
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Added && c.is_public)
        .count();
    let existing_functions_modified = scan
        .changes
        .iter()
        .filter(|c| {
            c.symbol_kind == NodeKind::Function
                && matches!(
                    c.kind,
                    ChangeKind::SignatureChanged | ChangeKind::BodyOnly | ChangeKind::DocOnly
                )
        })
        .count();
    let added_functions: HashSet<SymbolKey<'_>> = scan
        .changes
        .iter()
        .filter(|change| {
            change.kind == ChangeKind::Added && change.symbol_kind == NodeKind::Function
        })
        .map(|change| (change.file.as_str(), change.name.as_str()))
        .collect();
    let consumers = added_function_consumers(&scan.head_indices, &added_functions);
    let single_consumer_helpers = scan
        .head_indices
        .iter()
        .flat_map(|index| &index.definitions)
        .filter(|d| {
            d.kind == NodeKind::Function
                && !d.is_public
                && !d.in_test_context
                && added_functions.contains(&(d.file_path.as_str(), d.name.as_str()))
                && consumers
                    .get(&(d.file_path.as_str(), d.name.as_str()))
                    .is_some_and(|c| c.len() == 1)
        })
        .count();
    let single_consumer_modules = added_files
        .iter()
        .filter(|file| {
            module_has_one_external_consumer(file, &scan.head_indices, &consumers, &added_files)
        })
        .count();
    let denominator = functions_added + existing_functions_modified;

    SprawlLedger {
        source_files_added: added_files.len(),
        functions_added,
        public_symbols_added,
        existing_functions_modified,
        single_consumer_helpers,
        single_consumer_modules,
        creation_bias: if denominator == 0 {
            0.0
        } else {
            ((functions_added as f64 / denominator as f64) * 100.0).round() / 100.0
        },
    }
}

type SymbolKey<'a> = (&'a str, &'a str);
type Consumers<'a> = HashMap<SymbolKey<'a>, HashSet<SymbolKey<'a>>>;

/// Consumers of names that are unambiguous among newly added definitions.
fn added_function_consumers<'a>(
    indices: &'a [FileIndex],
    added: &HashSet<SymbolKey<'a>>,
) -> Consumers<'a> {
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for definition in indices.iter().flat_map(|index| &index.definitions) {
        if definition.kind == NodeKind::Function {
            *name_counts.entry(&definition.name).or_default() += 1;
        }
    }
    let mut definitions: HashMap<&str, Vec<&Definition>> = HashMap::new();
    for definition in indices.iter().flat_map(|index| &index.definitions) {
        if definition.kind == NodeKind::Function
            && added.contains(&(definition.file_path.as_str(), definition.name.as_str()))
        {
            definitions
                .entry(&definition.name)
                .or_default()
                .push(definition);
        }
    }
    let unique: HashMap<&str, &Definition> = definitions
        .into_iter()
        .filter_map(|(name, defs)| {
            (defs.len() == 1 && name_counts.get(name) == Some(&1)).then_some((name, defs[0]))
        })
        .collect();
    let mut consumers: Consumers<'_> = HashMap::new();
    for index in indices {
        for reference in &index.references {
            if !matches!(reference.kind, ReferenceKind::Call | ReferenceKind::Value) {
                continue;
            }
            let Some(target) = unique.get(reference.name.as_str()) else {
                continue;
            };
            let Some(owner) = containing_function(index, reference.line) else {
                continue;
            };
            if owner.file_path == target.file_path && owner.name == target.name {
                continue;
            }
            consumers
                .entry((&target.file_path, &target.name))
                .or_default()
                .insert((&owner.file_path, &owner.name));
        }
    }
    consumers
}

fn containing_function(index: &FileIndex, line: u32) -> Option<&Definition> {
    index
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function && d.line_start <= line && line <= d.line_end)
        .min_by_key(|d| d.line_end - d.line_start)
}

fn module_has_one_external_consumer(
    file: &str,
    indices: &[FileIndex],
    consumers: &Consumers<'_>,
    added_files: &HashSet<&str>,
) -> bool {
    let functions: Vec<&Definition> = indices
        .iter()
        .filter(|index| index.file_path == file)
        .flat_map(|index| &index.definitions)
        .filter(|d| d.kind == NodeKind::Function && !d.in_test_context)
        .collect();
    if functions.is_empty() {
        return false;
    }
    let external: HashSet<&str> = functions
        .iter()
        .flat_map(|d| {
            consumers
                .get(&(d.file_path.as_str(), d.name.as_str()))
                .into_iter()
                .flatten()
        })
        .map(|(consumer_file, _)| *consumer_file)
        .filter(|consumer_file| *consumer_file != file && !added_files.contains(consumer_file))
        .collect();
    external.len() == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_util::BlobParser;
    use crate::review::{ContractChange, UnanalyzedFile};
    use std::collections::{BTreeMap, HashSet};

    fn change(name: &str, file: &str, kind: ChangeKind, public: bool) -> ContractChange {
        ContractChange {
            name: name.into(),
            symbol_kind: NodeKind::Function,
            file: file.into(),
            kind,
            sig_base: None,
            sig_head: None,
            hash_base: None,
            hash_head: None,
            is_public: public,
            callers_outside_diff: vec![],
            callers_outside_diff_count: 0,
        }
    }

    #[test]
    fn measures_creation_bias_and_single_consumers() {
        let mut parser = BlobParser::new();
        let new_file = parser
            .parse(
                "src/time.rs",
                "fn helper(value: &str) -> i64 { value.len() as i64 }\n\
                 pub fn api(value: &str) -> i64 { helper(value) }\n",
            )
            .unwrap();
        let caller = parser
            .parse(
                "src/app.rs",
                "fn legacy(value: &str) -> i64 { value.len() as i64 }\n\
                 fn existing(value: &str) -> i64 { legacy(value) + api(value) }\n",
            )
            .unwrap();
        let scan = DiffScan {
            changes: vec![
                change("helper", "src/time.rs", ChangeKind::Added, false),
                change("api", "src/time.rs", ChangeKind::Added, true),
                change("existing", "src/app.rs", ChangeKind::BodyOnly, false),
            ],
            unanalyzed: Vec::<UnanalyzedFile>::new(),
            diff_files: HashSet::new(),
            files_analyzed: 2,
            base_indices: vec![],
            head_indices: vec![new_file, caller],
            renames: BTreeMap::new(),
        };
        let paths = vec![
            ChangedPath {
                path: "src/time.rs".into(),
                status: ChangeStatus::Added,
            },
            ChangedPath {
                path: "src/app.rs".into(),
                status: ChangeStatus::Modified,
            },
        ];

        assert_eq!(
            measure(&paths, &scan),
            SprawlLedger {
                source_files_added: 1,
                functions_added: 2,
                public_symbols_added: 1,
                existing_functions_modified: 1,
                single_consumer_helpers: 1,
                single_consumer_modules: 1,
                creation_bias: 0.67,
            }
        );
    }
}
