//! `keel map --semantic` — deterministic per-module semantic enrichment.
//!
//! For each module we emit a one-line summary, its public functions and
//! types, and a deterministic `when_to_use` hint. Everything is derived from
//! the stored graph and the docstrings the parsers already extracted — there
//! is NO LLM generation and NO fabricated prose.

use serde::{Deserialize, Serialize};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, EdgeKind, NodeKind};

/// Split snake/camel/path text into unique lowercase identifier words.
///
/// `min_len` is applied after splitting, so `toUnixSeconds` still contributes
/// `unix` and `seconds` even though the leading `to` is discarded.
pub(crate) fn identifier_words(text: &str, min_len: usize) -> std::collections::BTreeSet<String> {
    let mut words = std::collections::BTreeSet::new();
    let mut current = String::new();
    let mut previous_lower = false;
    let flush = |word: &mut String, out: &mut std::collections::BTreeSet<String>| {
        if word.len() >= min_len {
            out.insert(std::mem::take(word));
        } else {
            word.clear();
        }
    };
    for character in text.chars() {
        if character.is_alphanumeric() {
            if character.is_uppercase() && previous_lower {
                flush(&mut current, &mut words);
            }
            current.extend(character.to_lowercase());
            previous_lower = character.is_lowercase();
        } else {
            flush(&mut current, &mut words);
            previous_lower = false;
        }
    }
    flush(&mut current, &mut words);
    words
}

/// A public symbol in a module: name, signature, and content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSymbol {
    pub name: String,
    pub signature: String,
    pub hash: String,
}

/// Semantic view of a single module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticModule {
    pub path: String,
    /// First docstring line (module doc, else first public symbol's doc).
    pub summary: String,
    pub public_functions: Vec<SemanticSymbol>,
    pub public_types: Vec<SemanticSymbol>,
    /// Deterministic usage hint (imports/importers/exports), never prose.
    pub when_to_use: String,
}

/// The semantic map payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMapResult {
    pub version: String,
    pub command: String,
    pub modules: Vec<SemanticModule>,
}

/// First non-empty trimmed line of a docstring, if any.
fn first_line(doc: &str) -> Option<String> {
    doc.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from)
}

fn symbol(node: &keel_core::types::GraphNode) -> SemanticSymbol {
    SemanticSymbol {
        name: node.name.clone(),
        signature: node.signature.clone(),
        hash: node.hash.clone(),
    }
}

/// Build the semantic map from the stored graph.
pub fn build_semantic_map(store: &dyn GraphStore) -> SemanticMapResult {
    let mut modules: Vec<SemanticModule> = Vec::new();

    for module in store.get_all_modules() {
        let nodes = store.get_nodes_in_file(&module.file_path);

        let mut public_functions = Vec::new();
        let mut public_types = Vec::new();
        let mut first_symbol_doc: Option<String> = None;

        for n in &nodes {
            if !n.is_public {
                continue;
            }
            match n.kind {
                NodeKind::Function => {
                    if first_symbol_doc.is_none() {
                        first_symbol_doc = n.docstring.as_deref().and_then(first_line);
                    }
                    public_functions.push(symbol(n));
                }
                NodeKind::Class => {
                    if first_symbol_doc.is_none() {
                        first_symbol_doc = n.docstring.as_deref().and_then(first_line);
                    }
                    public_types.push(symbol(n));
                }
                NodeKind::Module => {}
            }
        }

        let summary = module
            .docstring
            .as_deref()
            .and_then(first_line)
            .or(first_symbol_doc)
            .unwrap_or_default();

        // Deterministic when_to_use: imports / importers / top exports.
        let import_count = store
            .get_module_profile(module.id)
            .map(|p| p.import_sources.len())
            .unwrap_or(0);
        let importer_count = store
            .get_edges(module.id, EdgeDirection::Incoming)
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports)
            .count();
        let top_exports: Vec<String> = public_functions
            .iter()
            .chain(public_types.iter())
            .take(3)
            .map(|s| s.name.clone())
            .collect();
        let exports = if top_exports.is_empty() {
            "(none)".to_string()
        } else {
            top_exports.join(", ")
        };
        let when_to_use = format!(
            "imports {} modules, imported by {} files, exports: {}",
            import_count, importer_count, exports
        );

        modules.push(SemanticModule {
            path: module.file_path.clone(),
            summary,
            public_functions,
            public_types,
            when_to_use,
        });
    }

    modules.sort_by(|a, b| a.path.cmp(&b.path));

    SemanticMapResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "map".to_string(),
        modules,
    }
}

#[cfg(test)]
#[path = "semantic_tests.rs"]
mod tests;
