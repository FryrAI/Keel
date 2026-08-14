//! Head-diff and base-graph views used by reuse scoring.

use std::collections::{BTreeSet, HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeKind};
use keel_parsers::resolver::{Definition, FileIndex, ReferenceKind};

use super::super::diff::DiffScan;
use super::super::ChangeKind;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub(super) struct SymbolKey {
    pub(super) file: String,
    pub(super) name: String,
}

#[derive(Clone)]
pub(super) struct CallUse {
    pub(super) owner: SymbolKey,
    pub(super) line: u32,
    pub(super) offset: u32,
}

pub(super) struct HeadGraph<'a> {
    pub(super) added: Vec<&'a Definition>,
    pub(super) callers: HashMap<SymbolKey, Vec<CallUse>>,
    pub(super) calls: HashMap<SymbolKey, Vec<(String, u32, u32, bool)>>,
}

pub(super) struct BaseGraph {
    pub(super) by_id: HashMap<u64, GraphNode>,
    by_key: HashMap<SymbolKey, u64>,
    pub(super) by_name: HashMap<String, Vec<u64>>,
    pub(super) calls: HashMap<SymbolKey, Vec<(String, u32, u32, bool)>>,
    pub(super) callers_by_name: HashMap<String, Vec<CallUse>>,
    pub(super) changed_files: HashSet<String>,
}

#[derive(Default)]
pub(super) struct Role {
    pub(super) caller_files: HashSet<String>,
    pub(super) callee_names: HashSet<String>,
}

impl SymbolKey {
    /// Build a stable diff-local key for a parsed definition.
    pub(super) fn of_definition(definition: &Definition) -> Self {
        Self {
            file: definition.file_path.clone(),
            name: definition.name.clone(),
        }
    }

    fn of_node(node: &GraphNode) -> Self {
        Self {
            file: node.file_path.clone(),
            name: node.name.clone(),
        }
    }
}

impl HeadGraph<'_> {
    /// Index calls involving functions added in the head diff.
    pub(super) fn build(scan: &DiffScan) -> HeadGraph<'_> {
        let added_keys: HashSet<SymbolKey> = scan
            .changes
            .iter()
            .filter(|change| {
                change.kind == ChangeKind::Added && change.symbol_kind == NodeKind::Function
            })
            .map(|change| SymbolKey {
                file: change.file.clone(),
                name: change.name.clone(),
            })
            .collect();
        let added: Vec<&Definition> = scan
            .head_indices
            .iter()
            .flat_map(|index| &index.definitions)
            .filter(|definition| added_keys.contains(&SymbolKey::of_definition(definition)))
            .collect();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for definition in scan
            .head_indices
            .iter()
            .flat_map(|index| &index.definitions)
        {
            *counts.entry(&definition.name).or_default() += 1;
        }
        let unique_added: HashMap<&str, SymbolKey> = added
            .iter()
            .filter(|definition| counts.get(definition.name.as_str()) == Some(&1))
            .map(|definition| {
                (
                    definition.name.as_str(),
                    SymbolKey::of_definition(definition),
                )
            })
            .collect();
        let mut callers: HashMap<SymbolKey, Vec<CallUse>> = HashMap::new();
        let mut calls: HashMap<SymbolKey, Vec<(String, u32, u32, bool)>> = HashMap::new();
        for index in &scan.head_indices {
            for reference in &index.references {
                if reference.kind != ReferenceKind::Call {
                    continue;
                }
                let Some(owner) = role_containing_function(index, reference.line) else {
                    continue;
                };
                let owner_key = SymbolKey::of_definition(owner);
                let called_name = role_bare_name(&reference.name).to_string();
                calls.entry(owner_key.clone()).or_default().push((
                    called_name.clone(),
                    reference.line,
                    reference.line.saturating_sub(owner.line_start),
                    role_eligible(reference),
                ));
                if let Some(target) = unique_added.get(called_name.as_str()) {
                    if owner_key != *target {
                        callers.entry(target.clone()).or_default().push(CallUse {
                            owner: owner_key,
                            line: reference.line,
                            offset: reference.line.saturating_sub(owner.line_start),
                        });
                    }
                }
            }
        }
        HeadGraph {
            added,
            callers,
            calls,
        }
    }
}

impl BaseGraph {
    /// Load stored functions and overlay parsed base-side calls for changed files.
    pub(super) fn load(store: &dyn GraphStore, scan: &DiffScan) -> Self {
        let paths: BTreeSet<String> = store
            .get_all_modules()
            .into_iter()
            .map(|module| module.file_path)
            .collect();
        let mut by_id = HashMap::new();
        let mut by_key = HashMap::new();
        let mut by_name: HashMap<String, Vec<u64>> = HashMap::new();
        let added_keys: HashSet<SymbolKey> = scan
            .changes
            .iter()
            .filter(|change| {
                change.kind == ChangeKind::Added && change.symbol_kind == NodeKind::Function
            })
            .map(|change| SymbolKey {
                file: change.file.clone(),
                name: change.name.clone(),
            })
            .collect();
        let nodes: Vec<GraphNode> = paths
            .iter()
            .flat_map(|path| store.get_nodes_in_file(path))
            .filter(|node| {
                node.kind == NodeKind::Function && !added_keys.contains(&SymbolKey::of_node(node))
            })
            .collect();
        let mut key_counts: HashMap<SymbolKey, usize> = HashMap::new();
        for node in &nodes {
            *key_counts.entry(SymbolKey::of_node(node)).or_default() += 1;
        }
        for node in nodes {
            let key = SymbolKey::of_node(&node);
            if key_counts.get(&key) == Some(&1) {
                by_key.insert(key, node.id);
            }
            by_name.entry(node.name.clone()).or_default().push(node.id);
            by_id.insert(node.id, node);
        }
        let mut calls: HashMap<SymbolKey, Vec<(String, u32, u32, bool)>> = HashMap::new();
        let mut callers_by_name: HashMap<String, Vec<CallUse>> = HashMap::new();
        for index in &scan.base_indices {
            for reference in &index.references {
                if reference.kind != ReferenceKind::Call {
                    continue;
                }
                let Some(owner) = role_containing_function(index, reference.line) else {
                    continue;
                };
                let owner_key = SymbolKey::of_definition(owner);
                let called_name = role_bare_name(&reference.name).to_string();
                let use_site = CallUse {
                    owner: owner_key.clone(),
                    line: reference.line,
                    offset: reference.line.saturating_sub(owner.line_start),
                };
                calls.entry(owner_key).or_default().push((
                    called_name.clone(),
                    reference.line,
                    use_site.offset,
                    role_eligible(reference),
                ));
                callers_by_name
                    .entry(called_name)
                    .or_default()
                    .push(use_site);
            }
        }
        BaseGraph {
            by_id,
            by_key,
            by_name,
            calls,
            callers_by_name,
            changed_files: scan.diff_files.clone(),
        }
    }

    /// Resolve a parsed head owner to its corresponding base-side key.
    pub(super) fn key_for_head_owner(&self, scan: &DiffScan, owner: &SymbolKey) -> SymbolKey {
        let file = scan.renames.get(&owner.file).unwrap_or(&owner.file);
        SymbolKey {
            file: file.clone(),
            name: owner.name.clone(),
        }
    }

    /// Resolve a unique parsed/stored key to its stored node.
    pub(super) fn node_for_key(&self, key: &SymbolKey) -> Option<&GraphNode> {
        self.by_key.get(key).and_then(|id| self.by_id.get(id))
    }
}

fn role_containing_function(index: &FileIndex, line: u32) -> Option<&Definition> {
    index
        .definitions
        .iter()
        .filter(|definition| {
            definition.kind == NodeKind::Function
                && definition.line_start <= line
                && line <= definition.line_end
        })
        .min_by_key(|definition| definition.line_end - definition.line_start)
}

fn role_bare_name(name: &str) -> &str {
    name.rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(name)
}

fn role_eligible(reference: &keel_parsers::resolver::Reference) -> bool {
    reference.resolved_to.is_some() || !reference.name.contains(['.', ':'])
}
