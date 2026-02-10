use keel_output::OutputFormatter;

/// Run `keel stats` — display telemetry dashboard.
pub fn run(_formatter: &dyn OutputFormatter, verbose: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel stats: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel stats: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(
        db_path.to_str().unwrap_or(""),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel stats: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Gather basic stats from the graph store
    let modules = keel_core::store::GraphStore::get_all_modules(&store);
    let module_count = modules.len();

    let mut function_count = 0u32;
    let mut file_set = std::collections::HashSet::new();
    for module in &modules {
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        function_count += nodes.iter().filter(|n| n.kind == keel_core::types::NodeKind::Function).count() as u32;
        file_set.insert(module.file_path.clone());
    }

    // Count edges by kind — query all nodes, not just modules
    let mut calls_count = 0u32;
    let mut imports_count = 0u32;
    let mut contains_count = 0u32;
    let mut seen_edges = std::collections::HashSet::new();
    for module in &modules {
        // Get edges from module and from all nodes in the file
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        let all_node_ids: Vec<u64> = std::iter::once(module.id)
            .chain(nodes.iter().map(|n| n.id))
            .collect();
        for nid in all_node_ids {
            let edges = keel_core::store::GraphStore::get_edges(
                &store,
                nid,
                keel_core::types::EdgeDirection::Outgoing,
            );
            for edge in &edges {
                if seen_edges.insert(edge.id) {
                    match edge.kind {
                        keel_core::types::EdgeKind::Calls => calls_count += 1,
                        keel_core::types::EdgeKind::Imports => imports_count += 1,
                        keel_core::types::EdgeKind::Contains => contains_count += 1,
                        _ => {}
                    }
                }
            }
        }
    }

    println!("keel stats");
    println!("  modules:   {}", module_count);
    println!("  functions: {}", function_count);
    println!("  files:     {}", file_set.len());
    println!("  edges:");
    println!("    calls:    {}", calls_count);
    println!("    imports:  {}", imports_count);
    println!("    contains: {}", contains_count);

    if verbose {
        println!("  db_path:   {}", db_path.display());
        if let Ok(v) = store.schema_version() {
            println!("  schema:    v{}", v);
        }
    }

    0
}
