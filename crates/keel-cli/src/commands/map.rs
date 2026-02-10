use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

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

/// Run `keel map` — full re-parse of the codebase.
pub fn run(
    formatter: &dyn OutputFormatter,
    verbose: bool,
    _llm_verbose: bool,
    _scope: Option<String>,
    _strict: bool,
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

    // Full re-map: clear existing graph data so IDs start fresh
    if let Err(e) = store.clear_all() {
        eprintln!("keel map: failed to clear graph database: {}", e);
        return 2;
    }

    // Walk all source files
    let walker = FileWalker::new(&cwd);
    let entries = walker.walk();

    if verbose {
        eprintln!("keel map: found {} source files", entries.len());
    }

    // Create resolvers for each language
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go_resolver = GoResolver::new();
    let rs = RustLangResolver::new();

    // Disable FK enforcement for bulk operations (re-enabled after)
    let _ = store.set_foreign_keys(false);

    // Clear existing data for a full re-map
    if let Err(e) = store.clear_all() {
        eprintln!("keel map: failed to clear existing data: {}", e);
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

    // === Second pass: cross-file call edges and import edges ===
    for file_data in &all_file_data {
        let file_path = &file_data.file_path;

        // Create Imports edges between modules
        if let Some(&src_module_id) = file_module_ids.get(file_path.as_str()) {
            for imp in &file_data.imports {
                // Try to match the import source to a known file module
                let imp_source = &imp.source;
                if let Some(&tgt_module_id) = file_module_ids.get(imp_source.as_str()) {
                    let edge_id = next_id;
                    next_id += 1;
                    edge_changes.push(EdgeChange::Add(GraphEdge {
                        id: edge_id,
                        source_id: src_module_id,
                        target_id: tgt_module_id,
                        kind: EdgeKind::Imports,
                        file_path: file_path.clone(),
                        line: imp.line,
                    }));
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
            let target_id = resolve_cross_file_call(
                &reference.name,
                &file_data.imports,
                &global_name_index,
                &file_module_ids,
            );

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
                        }));
                    }
                }
            }
        }
    }

    // Collect valid node IDs
    let valid_node_ids: std::collections::HashSet<u64> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) => Some(n.id),
            _ => None,
        })
        .collect();

    // Filter out edges referencing non-existent nodes
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

    // Gather stats from node_changes and valid_edges BEFORE consuming them
    let total_edges = valid_edges.iter().filter(|e| matches!(e, EdgeChange::Add(_))).count() as u32;
    let map_result = build_map_result(&node_changes, &valid_edges, &entries);

    if verbose {
        eprintln!("keel map: inserting {} nodes", node_changes.len());
    }

    // Apply node changes
    if let Err(e) = store.update_nodes(node_changes) {
        eprintln!("keel map: failed to update nodes: {}", e);
        return 2;
    }

    // Apply edge changes (using valid_edges filtered above)
    if let Err(e) = store.update_edges(valid_edges) {
        eprintln!("keel map: failed to update edges: {}", e);
        return 2;
    }

    // Re-enable FK enforcement
    let _ = store.set_foreign_keys(true);

    if verbose {
        eprintln!("keel map: mapped {} files, {} edges", entries.len(), total_edges);
    }

    let output = formatter.format_map(&map_result);
    if !output.is_empty() {
        println!("{}", output);
    }
    0
}

/// Build a MapResult from collected node and edge data (before they are consumed).
fn build_map_result(
    node_changes: &[NodeChange],
    valid_edges: &[EdgeChange],
    entries: &[keel_parsers::walker::WalkEntry],
) -> keel_enforce::types::MapResult {
    use keel_enforce::types::{MapResult, MapSummary, ModuleEntry};

    let nodes: Vec<_> = node_changes
        .iter()
        .filter_map(|c| match c {
            NodeChange::Add(n) => Some(n),
            _ => None,
        })
        .collect();

    let total_nodes = nodes.len() as u32;
    let total_edges = valid_edges.iter().filter(|e| matches!(e, EdgeChange::Add(_))).count() as u32;
    let modules_count = nodes.iter().filter(|n| n.kind == NodeKind::Module).count() as u32;
    let functions_count = nodes.iter().filter(|n| n.kind == NodeKind::Function).count() as u32;
    let classes_count = nodes.iter().filter(|n| n.kind == NodeKind::Class).count() as u32;

    let non_module_nodes: Vec<_> = nodes.iter().filter(|n| n.kind != NodeKind::Module).collect();
    let type_hint_count = non_module_nodes.iter().filter(|n| n.type_hints_present).count();
    let docstring_count = non_module_nodes.iter().filter(|n| n.has_docstring).count();
    let type_hint_coverage = if non_module_nodes.is_empty() {
        0.0
    } else {
        type_hint_count as f64 / non_module_nodes.len() as f64
    };
    let docstring_coverage = if non_module_nodes.is_empty() {
        0.0
    } else {
        docstring_count as f64 / non_module_nodes.len() as f64
    };

    let mut languages: HashSet<String> = HashSet::new();
    for entry in entries {
        languages.insert(entry.language.clone());
    }
    let mut langs: Vec<String> = languages.into_iter().collect();
    langs.sort();

    let external_endpoint_count = nodes.iter().map(|n| n.external_endpoints.len()).sum::<usize>() as u32;

    // Per-module entries: count functions, classes, edges per module
    let mut module_entries = Vec::new();
    for node in &nodes {
        if node.kind != NodeKind::Module {
            continue;
        }
        let module_id = node.id;
        let file_path = &node.file_path;

        let fn_count = nodes
            .iter()
            .filter(|n| n.module_id == module_id && n.kind == NodeKind::Function)
            .count() as u32;
        let cls_count = nodes
            .iter()
            .filter(|n| n.module_id == module_id && n.kind == NodeKind::Class)
            .count() as u32;
        let edge_count = valid_edges
            .iter()
            .filter(|e| match e {
                EdgeChange::Add(edge) => &edge.file_path == file_path,
                _ => false,
            })
            .count() as u32;

        module_entries.push(ModuleEntry {
            path: file_path.clone(),
            function_count: fn_count,
            class_count: cls_count,
            edge_count,
            responsibility_keywords: None,
            external_endpoints: None,
        });
    }

    MapResult {
        version: "0.1.0".to_string(),
        command: "map".to_string(),
        summary: MapSummary {
            total_nodes,
            total_edges,
            modules: modules_count,
            functions: functions_count,
            classes: classes_count,
            external_endpoints: external_endpoint_count,
            languages: langs,
            type_hint_coverage,
            docstring_coverage,
        },
        modules: module_entries,
    }
}

/// Make a path relative to the project root.
fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// Resolve a cross-file call reference by matching imports to the global name index.
fn resolve_cross_file_call(
    callee_name: &str,
    imports: &[keel_parsers::resolver::Import],
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
    file_module_ids: &HashMap<String, u64>,
) -> Option<u64> {
    // Check if any import brings this name into scope
    for imp in imports {
        let names_match = imp.imported_names.iter().any(|n| n == callee_name || n == "*");
        if !names_match {
            continue;
        }
        // Find the target definition in the imported module
        if let Some(candidates) = global_name_index.get(callee_name) {
            // Prefer candidates from the import's source file
            let source = &imp.source;
            for (file, node_id) in candidates {
                if file == source {
                    return Some(*node_id);
                }
            }
            // Fallback: check if any candidate's file matches as a module
            for (file, node_id) in candidates {
                if file_module_ids.contains_key(file.as_str()) && source.contains(file.as_str()) {
                    return Some(*node_id);
                }
            }
            // Last resort: if only one candidate exists globally, use it
            if candidates.len() == 1 {
                return Some(candidates[0].1);
            }
        }
    }
    None
}

/// Find which definition contains a given line number.
fn find_containing_def(
    definitions: &[keel_parsers::resolver::Definition],
    line: u32,
    file_path: &str,
    name_to_id: &HashMap<(String, String), u64>,
) -> Option<u64> {
    for def in definitions {
        if line >= def.line_start && line <= def.line_end {
            return name_to_id
                .get(&(file_path.to_string(), def.name.clone()))
                .copied();
        }
    }
    None
}
