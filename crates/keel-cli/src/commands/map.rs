use std::collections::{HashMap, HashSet};
use std::fs;

use keel_core::hash::{compute_hash, compute_hash_disambiguated};
use keel_core::store::GraphStore;
use keel_core::types::{
    EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind,
};
use keel_output::OutputFormatter;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::typescript::TsResolver;
use keel_parsers::walker::FileWalker;

use super::map_helpers::{build_map_result, build_module_profiles, make_relative, populate_hotspots, populate_functions};
use super::map_resolve::{
    build_package_node_index, find_containing_def, resolve_cross_file_call,
    resolve_import_to_module, resolve_package_import, resolve_same_directory_call,
};

/// Run `keel map` — full re-parse of the codebase.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    _llm_verbose: bool,
    _scope: Option<String>,
    _strict: bool,
    _depth: u32,
    tier3_enabled: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel map: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel map: not initialized. Run `keel init` first.");
        return 2;
    }

    // Open graph store
    let db_path = keel_dir.join("graph.db");
    let mut store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel map: failed to open graph database: {}", e);
            return 2;
        }
    };

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

    // Create resolvers for each language
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go_resolver = GoResolver::new();
    let rs = RustLangResolver::new();

    // Disable FK enforcement for bulk operations (re-enabled after)
    if let Err(e) = store.set_foreign_keys(false) {
        eprintln!("keel map: WARNING: set_foreign_keys failed: {}", e);
    }

    // Full re-map: clear existing graph data so IDs start fresh
    if let Err(e) = store.clear_all() {
        eprintln!("keel map: failed to clear graph database: {}", e);
        return 2;
    }

    let mut node_changes = Vec::new();
    let mut edge_changes = Vec::new();
    let mut next_id = 1u64;
    // Map (file_path, name) -> node_id for building edges
    let mut name_to_id: HashMap<(String, String), u64> = HashMap::new();
    // Global name index: name -> [(file_path, node_id)] for cross-file resolution
    let mut global_name_index: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    // Per-file module IDs for creating Imports edges
    let mut file_module_ids: HashMap<String, u64> = HashMap::new();
    // Track assigned hashes to detect collisions
    let mut assigned_hashes: HashSet<String> = HashSet::new();
    // Track all valid node IDs for edge validation
    let mut valid_node_ids: HashSet<u64> = HashSet::new();

    // Collect per-file parse results for the cross-file second pass
    struct FileParseData {
        file_path: String,
        definitions: Vec<keel_parsers::resolver::Definition>,
        references: Vec<keel_parsers::resolver::Reference>,
        imports: Vec<keel_parsers::resolver::Import>,
    }
    let mut all_file_data: Vec<FileParseData> = Vec::new();

    // === First pass: create nodes and same-file edges ===
    for entry in &entries {
        let content = match fs::read_to_string(&entry.path) {
            Ok(c) => c,
            Err(e) => {
                if verbose {
                    eprintln!("keel map: skipping {}: {}", entry.path.display(), e);
                }
                continue;
            }
        };

        let resolver: &dyn LanguageResolver = match entry.language.as_str() {
            "typescript" | "javascript" | "tsx" => &ts,
            "python" => &py,
            "go" => &go_resolver,
            "rust" => &rs,
            _ => continue,
        };

        let result = resolver.parse_file(&entry.path, &content);
        let file_path = make_relative(&cwd, &entry.path);

        // Create module node for this file
        let module_id = next_id;
        next_id += 1;
        let module_hash = compute_hash(&file_path, "", "");
        assigned_hashes.insert(module_hash.clone());
        valid_node_ids.insert(module_id);
        file_module_ids.insert(file_path.clone(), module_id);
        node_changes.push(NodeChange::Add(GraphNode {
            id: module_id,
            hash: module_hash,
            kind: NodeKind::Module,
            name: file_path.clone(),
            signature: String::new(),
            file_path: file_path.clone(),
            line_start: 1,
            line_end: content.lines().count() as u32,
            docstring: None,
            is_public: true,
            type_hints_present: true,
            has_docstring: false,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
            package: entry.package.clone(),
        }));

        // Create definition nodes
        for def in &result.definitions {
            let mut hash = compute_hash(
                &def.signature,
                &def.body_text,
                def.docstring.as_deref().unwrap_or(""),
            );
            // If hash already assigned to another node, disambiguate with file_path
            if assigned_hashes.contains(&hash) {
                hash = compute_hash_disambiguated(
                    &def.signature,
                    &def.body_text,
                    def.docstring.as_deref().unwrap_or(""),
                    &file_path,
                );
            }
            assigned_hashes.insert(hash.clone());
            let node_id = next_id;
            next_id += 1;
            valid_node_ids.insert(node_id);

            name_to_id.insert((file_path.clone(), def.name.clone()), node_id);
            global_name_index
                .entry(def.name.clone())
                .or_default()
                .push((file_path.clone(), node_id));

            node_changes.push(NodeChange::Add(GraphNode {
                id: node_id,
                hash,
                kind: def.kind.clone(),
                name: def.name.clone(),
                signature: def.signature.clone(),
                file_path: file_path.clone(),
                line_start: def.line_start,
                line_end: def.line_end,
                docstring: def.docstring.clone(),
                is_public: def.is_public,
                type_hints_present: def.type_hints_present,
                has_docstring: def.docstring.is_some(),
                external_endpoints: vec![],
                previous_hashes: vec![],
                module_id,
                package: entry.package.clone(),
            }));

            // "contains" edge from module to definition
            let edge_id = next_id;
            next_id += 1;
            edge_changes.push(EdgeChange::Add(GraphEdge {
                id: edge_id,
                source_id: module_id,
                target_id: node_id,
                kind: EdgeKind::Contains,
                file_path: file_path.clone(),
                line: def.line_start,
                confidence: 1.0,
            }));
        }

        // Create same-file call edges from references
        for reference in &result.references {
            if reference.kind == keel_parsers::resolver::ReferenceKind::Call {
                // Try to find the target in the same file
                if let Some(&target_id) =
                    name_to_id.get(&(file_path.clone(), reference.name.clone()))
                {
                    let source_id = find_containing_def(
                        &result.definitions,
                        reference.line,
                        &file_path,
                        &name_to_id,
                    );
                    if let Some(src_id) = source_id {
                        if src_id != target_id {
                            let edge_id = next_id;
                            next_id += 1;
                            edge_changes.push(EdgeChange::Add(GraphEdge {
                                id: edge_id,
                                source_id: src_id,
                                target_id,
                                kind: EdgeKind::Calls,
                                file_path: file_path.clone(),
                                line: reference.line,
                                confidence: 0.95, // same-file call, high confidence
                            }));
                        }
                    }
                }
            }
        }

        // Save parse data for cross-file second pass
        all_file_data.push(FileParseData {
            file_path,
            definitions: result.definitions,
            references: result.references,
            imports: result.imports,
        });
    }

    // Build file -> package mapping and cross-package index for monorepo resolution
    let file_packages: HashMap<String, String> = all_file_data
        .iter()
        .filter_map(|fd| {
            // Look up package from entries by matching file path
            entries
                .iter()
                .find(|e| make_relative(&cwd, &e.path) == fd.file_path)
                .and_then(|e| e.package.as_ref().map(|p| (fd.file_path.clone(), p.clone())))
        })
        .collect();
    let package_node_index = if config.monorepo.enabled {
        build_package_node_index(&global_name_index, &file_packages)
    } else {
        HashMap::new()
    };

    // === Second pass: cross-file call edges and import edges ===
    for file_data in &all_file_data {
        let file_path = &file_data.file_path;

        // Create Imports edges between modules
        if let Some(&src_module_id) = file_module_ids.get(file_path.as_str()) {
            for imp in &file_data.imports {
                let tgt_module_id = resolve_import_to_module(&imp.source, &file_module_ids);
                if let Some(tgt_id) = tgt_module_id {
                    if tgt_id != src_module_id {
                        let edge_id = next_id;
                        next_id += 1;
                        edge_changes.push(EdgeChange::Add(GraphEdge {
                            id: edge_id,
                            source_id: src_module_id,
                            target_id: tgt_id,
                            kind: EdgeKind::Imports,
                            file_path: file_path.clone(),
                            line: imp.line,
                            confidence: 1.0,
                        }));
                    }
                }
            }
        }

        // Resolve cross-file call references
        for reference in &file_data.references {
            if reference.kind != keel_parsers::resolver::ReferenceKind::Call {
                continue;
            }
            // Skip if already resolved same-file
            if name_to_id.contains_key(&(file_path.clone(), reference.name.clone())) {
                continue;
            }

            // Look through this file's imports to find the source module
            let mut target_id = resolve_cross_file_call(
                &reference.name,
                &file_data.imports,
                &global_name_index,
                &file_module_ids,
            );

            // Fallback: same-directory/same-package resolution (Go, Python)
            // In Go, all files in the same directory share a package namespace.
            if target_id.is_none() && !reference.name.contains('.') && !reference.name.contains("::") {
                target_id = resolve_same_directory_call(
                    &reference.name,
                    file_path,
                    &global_name_index,
                );
            }

            // Fallback: cross-package resolution (monorepo mode)
            let mut confidence = 0.80;
            if target_id.is_none() && !package_node_index.is_empty() {
                for imp in &file_data.imports {
                    if let Some((pkg_tgt, pkg_conf)) = resolve_package_import(
                        &reference.name,
                        &imp.source,
                        &package_node_index,
                    ) {
                        target_id = Some(pkg_tgt);
                        confidence = pkg_conf;
                        break;
                    }
                }
            }

            if let Some(tgt_id) = target_id {
                let source_id = find_containing_def(
                    &file_data.definitions,
                    reference.line,
                    file_path,
                    &name_to_id,
                );
                if let Some(src_id) = source_id {
                    if src_id != tgt_id {
                        let edge_id = next_id;
                        next_id += 1;
                        edge_changes.push(EdgeChange::Add(GraphEdge {
                            id: edge_id,
                            source_id: src_id,
                            target_id: tgt_id,
                            kind: EdgeKind::Calls,
                            file_path: file_path.clone(),
                            line: reference.line,
                            confidence,
                        }));
                    }
                }
            }
        }
    }

    // === Third pass: Tier 3 resolution for still-unresolved references ===
    if tier3_enabled || config.tier3.enabled {
        let tier3_data: Vec<_> = all_file_data
            .iter()
            .map(|fd| super::map_tier3::Tier3FileData {
                file_path: &fd.file_path,
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

    // Filter out edges referencing non-existent nodes (valid_node_ids built in first pass)
    let (valid_edges, invalid_edges): (Vec<_>, Vec<_>) = edge_changes
        .into_iter()
        .partition(|e| match e {
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

    // Gather stats from node_changes and valid_edges BEFORE consuming them
    let total_edges = valid_edges.iter().filter(|e| matches!(e, EdgeChange::Add(_))).count() as u32;
    let mut map_result = build_map_result(&node_changes, &valid_edges, &entries);
    map_result.depth = _depth;

    // Populate hotspots (depth >= 1) and function entries (depth >= 2)
    if _depth >= 1 { populate_hotspots(&mut map_result, &node_changes, &valid_edges); }
    if _depth >= 2 { populate_functions(&mut map_result, &node_changes, &valid_edges); }

    // Build module profiles from node data (before consuming node_changes)
    let module_profiles = build_module_profiles(&node_changes);

    // Apply node changes (modules sorted first to satisfy module_id FK)
    if let Err(e) = store.update_nodes(node_changes) {
        eprintln!("keel map: failed to update nodes: {}", e);
        return 2;
    }

    // Apply edge changes (using valid_edges filtered above; FK still OFF from line 72)
    if let Err(e) = store.update_edges(valid_edges) {
        eprintln!("keel map: failed to update edges: {}", e);
        return 2;
    }

    // Populate module profiles
    if let Err(e) = store.upsert_module_profiles(module_profiles) {
        if verbose {
            eprintln!("keel map: failed to upsert module profiles: {}", e);
        }
    }

    // Re-enable FK enforcement
    let _ = store.set_foreign_keys(true);

    // Cleanup any orphaned edges (source/target referencing deleted nodes)
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
        eprintln!("keel map: mapped {} files, {} edges", entries.len(), total_edges);
    }

    let output = formatter.format_map(&map_result);
    if !output.is_empty() {
        println!("{}", output);
    }
    0
}
