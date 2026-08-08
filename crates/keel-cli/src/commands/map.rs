use std::collections::{HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, NodeChange, NodeKind};
use keel_output::OutputFormatter;
use keel_parsers::boundary::BoundaryLiterals;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::typescript::TsResolver;
use keel_parsers::walker::FileWalker;

use keel_enforce::map::{
    build_map_result, build_module_profiles, populate_functions, populate_hotspots,
};

use super::map_passes;
use super::map_resolve::build_package_node_index;
use crate::telemetry_recorder::EventMetrics;
use keel_core::paths::make_relative;

/// Run `keel map` — full re-parse of the codebase.
#[allow(clippy::too_many_arguments)]
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    _depth: u32,
    tier3_enabled: bool,
    cached: bool,
    semantic: bool,
) -> (i32, EventMetrics) {
    let (cwd, mut store) = match super::open_store("map") {
        Ok(x) => x,
        Err(code) => return (code, EventMetrics::default()),
    };
    let keel_dir = keel_core::paths::keel_dir(&cwd);

    // One config load serves both the drift check and the walk below.
    let config = keel_core::config::KeelConfig::load(&keel_dir);

    // Detect (never rewrite — Principle 7) a binary/docs version mismatch.
    // At most one line, emitted once per invocation.
    super::version_drift::warn(&cwd, &config);

    // --cached: read from existing graph.db instead of re-parsing
    if cached {
        return super::map_cached::run_cached(&store, formatter, verbose, _depth);
    }

    // Walk all source files (with optional monorepo package annotation)
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

    // Load the persisted Tier 3 resolution cache BEFORE clear_all() wipes it,
    // so SCIP/LSP resolutions survive across runs. Off by default: skip the
    // read entirely unless a tier-3 provider is wanted.
    let tier3_wanted = tier3_enabled || config.tier3.enabled;
    let resolution_cache_seed = if tier3_wanted {
        store.load_resolution_cache()
    } else {
        Vec::new()
    };

    // Full re-map: clear existing graph data so IDs start fresh
    if let Err(e) = store.clear_all() {
        eprintln!("keel map: failed to clear graph database: {}", e);
        return (2, EventMetrics::default());
    }

    // The graph is now empty, so the previous run's map markers describe
    // nothing. Drop them before doing any work: if this run dies partway, the
    // graph must read as never-mapped rather than as freshly mapped at HEAD —
    // the latter satisfies `keel compile`'s staleness guard and would let a
    // compile enforce against an empty graph. They are re-stamped at the end.
    if let Err(e) = store.clear_map_markers() {
        eprintln!("keel map: failed to clear stale map markers: {}", e);
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

    // === Boundary providers: scan BEFORE the first pass ===
    // The symbols are materialised as nodes further down (after the first pass,
    // so node ids keep their established order), but their *names* are needed
    // now: they are the key set that decides which string literals survive
    // parsing as dispatch references (`ReferenceKind::Literal`). Scanned once
    // and reused for the injection below.
    let providers: Vec<Box<dyn keel_parsers::boundary::BoundaryProvider>> =
        vec![Box::new(keel_parsers::boundary::BamlProvider)];
    let scanned: Vec<(Vec<keel_parsers::boundary::BoundarySymbol>, f64)> = providers
        .iter()
        .map(|p| (p.scan(&cwd), p.confidence()))
        .collect();
    let literal_keys =
        super::map_boundary::literal_keys(scanned.iter().flat_map(|(symbols, _)| symbols));
    if !literal_keys.is_empty() {
        ts.set_boundary_literals(literal_keys.clone());
        py.set_boundary_literals(literal_keys.clone());
        rs.set_boundary_literals(literal_keys);
    }

    // === First pass: create nodes and same-file edges ===
    let mut body_index: Vec<keel_core::types::BodyIndexEntry> = Vec::new();
    let mut fragments = keel_core::fragments::FragmentScan::new();
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
        &mut fragments,
    );

    // === Boundary providers: materialise the declarations scanned above (from
    // surfaces keel has no grammar for — today BAML `.baml` functions/classes)
    // as boundary nodes so calls into them (e.g. `b.ExtractResume(...)`, or a
    // `"ExtractResume"` dispatch literal) resolve instead of reading as silent
    // unresolved edges. ===
    // `function name -> (node id, confidence)`: each provider's scanned symbols
    // enter the index at that provider's own confidence, so a boundary edge
    // records the tier of the provider that produced its target — no shared
    // scalar that the last provider in the loop would overwrite for every edge.
    let mut boundary_index: HashMap<String, (u64, f64)> = HashMap::new();
    for (symbols, confidence) in &scanned {
        if symbols.is_empty() {
            continue;
        }
        boundary_index.extend(super::map_boundary::inject_boundary_symbols(
            symbols,
            *confidence,
            &mut node_changes,
            &mut edge_changes,
            &mut next_id,
            &mut assigned_hashes,
            &mut valid_node_ids,
        ));
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
        ts: Some(&ts),
        py: Some(&py),
        go: Some(&go_resolver),
        rs: Some(&rs),
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
        &boundary_index,
        &mut edge_changes,
        &mut next_id,
        &mut node_tiers,
    );

    // === Third pass: Tier 3 resolution for still-unresolved references ===
    let mut resolution_cache_flush: Vec<keel_core::types::ResolutionCacheEntry> = Vec::new();
    if tier3_wanted {
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
            resolution_cache_seed,
            &mut resolution_cache_flush,
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

    // Refresh the fragment-clone measurements (issue #66) — also a full
    // rebuild, and for a stronger reason: whether a fragment is cloned depends
    // on every *other* body in the repo, so no subset of the tree can update
    // one row correctly.
    if let Err(e) = store.replace_fragment_clones(fragments.finish()) {
        eprintln!("keel map: failed to update fragment clones: {}", e);
    }

    // Persist the Tier 3 resolution cache for the next run. Skipped when tier-3
    // is off (nothing to flush) so the default path adds zero DB writes. A pure
    // perf optimization — warn and continue rather than fail the map.
    if tier3_wanted {
        if let Err(e) = store.replace_resolution_cache(resolution_cache_flush) {
            eprintln!("keel map: failed to persist resolution cache: {}", e);
        }
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

    // Stamp "this graph has been mapped". W009's bootstrap guard reads it to
    // tell an empty edge set apart from a graph that was never built — without
    // the marker, the first compile after `keel init` would report every
    // dependency in the repo as new.
    let mapped_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string();
    if let Err(e) = store.set_meta_value(keel_core::sqlite_meta::LAST_MAP_AT, &mapped_at) {
        if verbose {
            eprintln!("keel map: failed to record map timestamp: {}", e);
        }
    }

    // Stamp WHICH commit the graph describes. `keel compile`'s staleness guard
    // reads it and refuses to enforce once HEAD has moved somewhere this
    // commit is not an ancestor of. Outside a git repo (or before the first
    // commit) there is nothing to stamp and no staleness to detect.
    if let Some(head) = keel_enforce::gitdiff::head_commit(&cwd) {
        if let Err(e) = store.set_meta_value(keel_core::sqlite_meta::LAST_MAP_COMMIT, &head) {
            if verbose {
                eprintln!("keel map: failed to record map commit: {}", e);
            }
        }
    }

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
