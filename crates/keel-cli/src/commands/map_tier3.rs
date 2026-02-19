//! Tier 3 resolution pass for `keel map`.
//!
//! Resolves call references that Tier 1 (tree-sitter) and Tier 2 (per-language
//! enhancers) left unresolved, using SCIP indexes or LSP servers.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use keel_core::config::Tier3Config;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge};
use keel_parsers::resolver;

use super::map_resolve::find_containing_def;

/// File parse data needed for Tier 3 resolution.
pub(crate) struct Tier3FileData<'a> {
    pub file_path: &'a str,
    pub definitions: &'a [resolver::Definition],
    pub references: &'a [resolver::Reference],
}

/// Run the Tier 3 resolution pass over unresolved references.
///
/// Returns the number of newly resolved references and appends new edges
/// to `edge_changes`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_tier3_pass(
    config: &Tier3Config,
    languages: &[String],
    cwd: &Path,
    verbose: bool,
    file_data: &[Tier3FileData<'_>],
    name_to_id: &HashMap<(String, String), u64>,
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
    edge_changes: &mut Vec<EdgeChange>,
    next_id: &mut u64,
) -> u32 {
    let mut registry = keel_parsers::tier3::Tier3Registry::new();

    register_providers(&mut registry, config, languages, cwd, verbose);

    if registry.provider_count() == 0 {
        return 0;
    }

    let mut tier3_resolved = 0u32;
    for fd in file_data {
        for reference in fd.references {
            if reference.kind != resolver::ReferenceKind::Call {
                continue;
            }
            // Skip if already resolved (same-file or cross-file)
            if name_to_id.contains_key(&(fd.file_path.to_string(), reference.name.clone())) {
                continue;
            }
            // Check if earlier passes already created an edge at this location
            let already_has_edge = edge_changes.iter().any(|e| {
                if let EdgeChange::Add(edge) = e {
                    edge.file_path == fd.file_path && edge.line == reference.line
                } else {
                    false
                }
            });
            if already_has_edge {
                continue;
            }

            let call_site = resolver::CallSite {
                file_path: fd.file_path.to_string(),
                line: reference.line,
                callee_name: reference.name.clone(),
                receiver: None,
            };
            let result = registry.resolve(&call_site);
            if let keel_parsers::tier3::provider::Tier3Result::Resolved {
                target_file,
                target_name,
                confidence,
                ..
            } = result
            {
                if let Some(tgt_id) =
                    find_target_node(global_name_index, &target_file, &target_name)
                {
                    let source_id = find_containing_def(
                        fd.definitions,
                        reference.line,
                        fd.file_path,
                        name_to_id,
                    );
                    if let Some(src_id) = source_id {
                        if src_id != tgt_id {
                            let edge_id = *next_id;
                            *next_id += 1;
                            edge_changes.push(EdgeChange::Add(GraphEdge {
                                id: edge_id,
                                source_id: src_id,
                                target_id: tgt_id,
                                kind: EdgeKind::Calls,
                                file_path: fd.file_path.to_string(),
                                line: reference.line,
                                confidence,
                            }));
                            tier3_resolved += 1;
                        }
                    }
                }
            }
        }
    }

    if verbose && tier3_resolved > 0 {
        eprintln!(
            "keel map: tier3 resolved {} additional references",
            tier3_resolved
        );
    }

    registry.shutdown();
    tier3_resolved
}

/// Register SCIP and LSP providers based on configuration.
fn register_providers(
    registry: &mut keel_parsers::tier3::Tier3Registry,
    config: &Tier3Config,
    languages: &[String],
    cwd: &Path,
    verbose: bool,
) {
    #[cfg(feature = "tier3")]
    {
        use keel_parsers::tier3::provider::Tier3Provider;

        // Register SCIP providers from config
        for (lang, scip_path) in &config.scip_paths {
            let path = cwd.join(scip_path);
            let provider = keel_parsers::tier3::scip::ScipProvider::new(lang, path);
            if provider.is_available() {
                if verbose {
                    eprintln!("keel map: tier3 SCIP provider loaded for {}", lang);
                }
                registry.register(Box::new(provider));
            }
        }

        // Register LSP providers from config
        for (lang, cmd_args) in &config.lsp_commands {
            if let Some((cmd, args)) = cmd_args.split_first() {
                let provider =
                    keel_parsers::tier3::lsp::LspProvider::new(lang, cmd, args, cwd.to_path_buf());
                registry.register(Box::new(provider));
            }
        }

        // Register default LSP providers for languages without explicit config
        let configured_langs: HashSet<&str> = config
            .lsp_commands
            .keys()
            .chain(config.scip_paths.keys())
            .map(|s| s.as_str())
            .collect();
        for lang in languages {
            if !configured_langs.contains(lang.as_str()) {
                if let Some(provider) =
                    keel_parsers::tier3::lsp::LspProvider::from_defaults(lang, cwd.to_path_buf())
                {
                    registry.register(Box::new(provider));
                }
            }
        }
    }

    // Suppress unused variable warnings when tier3 feature is disabled
    #[cfg(not(feature = "tier3"))]
    {
        let _ = (config, languages, cwd, verbose);
    }
}

/// Find a target node ID by (file, name) in the global name index.
fn find_target_node(
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
    target_file: &str,
    target_name: &str,
) -> Option<u64> {
    global_name_index.get(target_name).and_then(|entries| {
        entries
            .iter()
            .find(|(f, _)| f == target_file)
            .or_else(|| entries.first())
            .map(|(_, id)| *id)
    })
}
