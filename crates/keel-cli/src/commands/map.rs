use std::collections::{HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, NodeChange, NodeKind};
use keel_output::OutputFormatter;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::typescript::TsResolver;
use keel_parsers::walker::FileWalker;

use super::map_helpers::{
    build_map_result, build_module_profiles, make_relative, populate_functions, populate_hotspots,
};
use super::map_passes;
use super::map_resolve::build_package_node_index;
use crate::telemetry_recorder::EventMetrics;

/// Run `keel map` — full re-parse of the codebase.
#[allow(clippy::too_many_arguments)]
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    _llm_verbose: bool,
    _scope: Option<String>,
    _strict: bool,
    _depth: u32,
    tier3_enabled: bool,
    cached: bool,
    semantic: bool,
) -> (i32, EventMetrics) {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel map: failed to get current directory: {}", e);
            return (2, EventMetrics::default());
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&cwd);
    if !keel_dir.exists() {
        eprintln!("keel map: not initialized. Run `keel init` first.");
        return (2, EventMetrics::default());
    }

    // Open graph store
    let db_path = keel_dir.join("graph.db");
    let mut store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or(""))
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel map: failed to open graph database: {}", e);
            return (2, EventMetrics::default());
        }
    };

    // --cached: read from existing graph.db instead of re-parsing
    if cached {
        return super::map_cached::run_cached(&store, formatter, verbose, _depth);
    }

    // Walk all source files (with optional monorepo package annotation)
    let config = keel_core::config::KeelConfig::load(&keel_dir);
    let walker = FileWalker::new(&cwd);
    let entries = if config.monorepo.enabled {
        let layout = keel_parsers::monorepo::detect_monorepo(&cwd);
        walker.walk_with_packages(&layout)
    } else {
        walker.walk()
    };

    if verbose {
        eprintln!("keel map: found {} source files", entries.len());
    }

    // Create resolvers for each language. `PyResolver::detect()` wires the
    // Tier-2 `ty` subprocess when it is on PATH and falls back to heuristics
    // otherwise.
    let ts = TsResolver::with_project_root(&cwd);
    let py = PyResolver::detect();
    let go_resolver = GoResolver::new();
    let rs = RustLangResolver::new();

    // Disable FK enforcement for bulk operations (re-enabled after)
    if let Err(e) = store.set_foreign_keys(false) {
        eprintln!("keel map: WARNING: set_foreign_keys failed: {}", e);
    }

    // Full re-map: clear existing graph data so IDs start fresh
    if let Err(e) = store.clear_all() {
        eprintln!("keel map: failed to clear graph database: {}", e);
        return (2, EventMetrics::default());
    }

    let mut node_changes = Vec::new();
    let mut edge_changes = Vec::new();
    let mut next_id = 1u64;
    let mut name_to_id: HashMap<(String, String), u64> = HashMap::new();
    let mut global_name_index: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    let mut file_module_ids: HashMap<String, u64> = HashMap::new();
    let mut assigned_hashes: HashSet<String> = HashSet::new();
    let mut valid_node_ids: HashSet<u64> = HashSet::new();

    // === First pass: create nodes and same-file edges ===
    let mut body_index: Vec<keel_core::types::BodyIndexEntry> = Vec::new();
    let all_file_data = map_passes::first_pass(
        &entries,
        &cwd,
        verbose,
        &ts,
        &py,
        &go_resolver,
        &rs,
        &mut node_changes,
        &mut edge_changes,
        &mut next_id,
        &mut name_to_id,
        &mut global_name_index,
        &mut file_module_ids,
        &mut assigned_hashes,
        &mut valid_node_ids,
        &mut body_index,
    );

    // === BAML boundary: materialise `.baml` function/class declarations as
    // boundary nodes so calls into them (e.g. `b.ExtractResume(...)`) resolve
    // instead of reading as silent unresolved edges. ===
    let baml_boundary = keel_parsers::baml::scan(&cwd);
    let baml_fn_index = super::map_baml::inject_baml_boundary(
        &baml_boundary,
        &mut node_changes,
        &mut edge_changes,
        &mut next_id,
        &mut assigned_hashes,
        &mut valid_node_ids,
    );
    if baml_boundary.baml_src_present && !baml_boundary.client_generated {
        eprintln!(
            "keel map: baml_src detected but no generated baml_client/baml_sdk found — run `baml generate` ({} BAML function(s) exposed as boundary stubs)",
            baml_fn_index.len()
        );
    }

    // Build file -> package mapping and cross-package index for monorepo resolution
    let file_packages: HashMap<String, String> = all_file_data
        .iter()
        .filter_map(|fd| {
            entries
                .iter()
                .find(|e| make_relative(&cwd, &e.path) == fd.file_path)
                .and_then(|e| {
                    e.package
                        .as_ref()
                        .map(|p| (fd.file_path.clone(), p.clone()))
                })
        })
        .collect();
    let package_node_index = if config.monorepo.enabled {
        build_package_node_index(&global_name_index, &file_packages)
    } else {
        HashMap::new()
    };

    // === Second pass: cross-file call edges and import edges ===
    // `node_tiers` records which resolution tier resolved each caller node's
    // outgoing edges, persisted to `nodes.resolution_tier` after the nodes land.
    let resolver_set = super::map_lang_resolve::ResolverSet {
        ts: &ts,
        py: &py,
        go: &go_resolver,
        rs: &rs,
    };
    let mut node_tiers: HashMap<u64, (String, f64)> = HashMap::new();
    map_passes::second_pass(
        &all_file_data,
        &cwd,
        &resolver_set,
        &name_to_id,
        &global_name_index,
        &file_module_ids,
        &package_node_index,
        &baml_fn_index,
        &mut edge_changes,
        &mut next_id,
        &mut node_tiers,
    );

    // === Third pass: Tier 3 resolution for still-unresolved references ===
    if tier3_enabled || config.tier3.enabled {
        let tier3_data: Vec<_> = all_file_data
            .iter()
            .map(|fd| super::map_tier3::Tier3FileData {
                file_path: &fd.file_path,
                module_id: file_module_ids.get(&fd.file_path).copied(),
                definitions: &fd.definitions,
                references: &fd.references,
            })
            .collect();
        super::map_tier3::run_tier3_pass(
            &config.tier3,
            &config.languages,
            &cwd,
            verbose,
            &tier3_data,
            &name_to_id,
            &global_name_index,
            &mut edge_changes,
            &mut next_id,
        );
    }

    // Filter out edges referencing non-existent nodes
    let (valid_edges, invalid_edges): (Vec<_>, Vec<_>) =
        edge_changes.into_iter().partition(|e| match e {
            EdgeChange::Add(edge) => {
                valid_node_ids.contains(&edge.source_id) && valid_node_ids.contains(&edge.target_id)
            }
            EdgeChange::Remove(_) => true,
        });

    if verbose && !invalid_edges.is_empty() {
        eprintln!(
            "keel map: filtered {} edges with invalid node references",
            invalid_edges.len()
        );
    }

    // Sort: modules first, then definitions (module_id FK dependency)
    node_changes.sort_by_key(|c| match c {
        NodeChange::Add(n) if n.kind == NodeKind::Module => 0,
        NodeChange::Add(_) => 1,
        NodeChange::Update(_) => 2,
        NodeChange::Remove(_) => 3,
    });

    // Gather stats BEFORE consuming changes
    let total_edges = valid_edges
        .iter()
        .filter(|e| matches!(e, EdgeChange::Add(_)))
        .count() as u32;

    let mut resolution_tiers: HashMap<String, u32> = HashMap::new();
    for edge in &valid_edges {
        if let EdgeChange::Add(e) = edge {
            let tier = if e.confidence >= 0.95 {
                "tier1"
            } else if e.confidence >= 0.80 {
                "tier2"
            } else {
                "tier3"
            };
            *resolution_tiers.entry(tier.to_string()).or_default() += 1;
        }
    }

    let mut map_result = build_map_result(&node_changes, &valid_edges, &entries);
    map_result.depth = _depth;

    if _depth >= 1 {
        populate_hotspots(&mut map_result, &node_changes, &valid_edges);
    }
    if _depth >= 2 {
        populate_functions(&mut map_result, &node_changes, &valid_edges);
    }

    let module_profiles = build_module_profiles(&node_changes);

    if let Err(e) = store.update_nodes(node_changes) {
        eprintln!("keel map: failed to update nodes: {}", e);
        return (2, EventMetrics::default());
    }

    // Persist the resolution tier that resolved each caller node's edges.
    let tiers: HashMap<u64, String> = node_tiers
        .into_iter()
        .map(|(id, (tier, _))| (id, tier))
        .collect();
    if let Err(e) = store.set_node_resolution_tiers(&tiers) {
        if verbose {
            eprintln!("keel map: failed to persist resolution tiers: {}", e);
        }
    }

    // Refresh the W006 duplicate-implementation index (full rebuild, like
    // the graph itself).
    if let Err(e) = store.replace_body_index(body_index) {
        eprintln!("keel map: failed to update body index: {}", e);
    }

    if let Err(e) = store.update_edges(valid_edges) {
        eprintln!("keel map: failed to update edges: {}", e);
        return (2, EventMetrics::default());
    }

    if let Err(e) = store.upsert_module_profiles(module_profiles) {
        if verbose {
            eprintln!("keel map: failed to upsert module profiles: {}", e);
        }
    }

    let _ = store.set_foreign_keys(true);

    match store.cleanup_orphaned_edges() {
        Ok(n) if n > 0 && verbose => {
            eprintln!("keel map: cleaned up {} orphaned edges", n);
        }
        Err(e) => {
            eprintln!("keel map: orphaned edge cleanup failed: {}", e);
        }
        _ => {}
    }

    if verbose {
        eprintln!(
            "keel map: mapped {} files, {} edges",
            entries.len(),
            total_edges
        );
    }

    // Build language mix from entries
    let mut lang_counts: HashMap<String, u32> = HashMap::new();
    for entry in &entries {
        *lang_counts.entry(entry.language.clone()).or_default() += 1;
    }
    let lang_total = lang_counts.values().sum::<u32>();
    let language_mix: HashMap<String, u32> = if lang_total > 0 {
        lang_counts
            .into_iter()
            .map(|(k, v)| (k, (v * 100) / lang_total))
            .collect()
    } else {
        HashMap::new()
    };

    let metrics = EventMetrics {
        node_count: map_result.summary.total_nodes,
        edge_count: total_edges,
        language_mix,
        resolution_tiers,
        ..Default::default()
    };

    // `--semantic`: emit deterministic per-module enrichment built from the
    // freshly-persisted graph instead of the standard map view.
    let output = if semantic {
        let sem = keel_enforce::semantic::build_semantic_map(&store);
        formatter.format_semantic_map(&sem)
    } else {
        formatter.format_map(&map_result)
    };
    if !output.is_empty() {
        println!("{}", output);
    }
    (0, metrics)
}
