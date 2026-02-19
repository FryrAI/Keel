use keel_output::OutputFormatter;

/// Run `keel stats` — display telemetry dashboard.
pub fn run(_formatter: &dyn OutputFormatter, verbose: bool, json: bool) -> i32 {
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
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
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
    let mut all_node_ids = Vec::new();
    for module in &modules {
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        function_count += nodes
            .iter()
            .filter(|n| n.kind == keel_core::types::NodeKind::Function)
            .count() as u32;
        file_set.insert(module.file_path.clone());
        for node in &nodes {
            all_node_ids.push(node.id);
        }
        all_node_ids.push(module.id);
    }

    // Count edges by kind
    let mut calls_count = 0u32;
    let mut imports_count = 0u32;
    let mut contains_count = 0u32;
    let mut seen_edges = std::collections::HashSet::new();
    for module in &modules {
        let nodes = keel_core::store::GraphStore::get_nodes_in_file(&store, &module.file_path);
        let all_ids: Vec<u64> = std::iter::once(module.id)
            .chain(nodes.iter().map(|n| n.id))
            .collect();
        for nid in all_ids {
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
    let edge_count = calls_count + imports_count + contains_count;

    // Load telemetry aggregate
    let telemetry_agg = load_telemetry_aggregate(&keel_dir);

    if json {
        let mut stats = serde_json::json!({
            "version": "0.1.0",
            "command": "stats",
            "modules": module_count,
            "functions": function_count,
            "files": file_set.len(),
            "edges": edge_count,
        });
        if let Some(ref agg) = telemetry_agg {
            stats["telemetry"] = serde_json::to_value(agg).unwrap_or_default();
        }
        if verbose {
            stats["db_path"] = serde_json::Value::String(db_path.display().to_string());
            if let Ok(v) = store.schema_version() {
                stats["schema_version"] = serde_json::Value::Number(v.into());
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&stats).unwrap_or_default()
        );
    } else {
        println!("keel stats");
        println!("  modules:   {}", module_count);
        println!("  functions: {}", function_count);
        println!("  files:     {}", file_set.len());
        println!("  edges:     {}", edge_count);
        println!("    calls:    {}", calls_count);
        println!("    imports:  {}", imports_count);
        println!("    contains: {}", contains_count);

        if verbose {
            println!("  db_path:   {}", db_path.display());
            if let Ok(v) = store.schema_version() {
                println!("  schema:    v{}", v);
            }
        }

        // Telemetry section
        if let Some(agg) = telemetry_agg {
            println!();
            println!("  telemetry (last 30 days):");
            println!("    invocations: {}", agg.total_invocations);
            if let Some(avg) = agg.avg_compile_ms {
                println!("    avg compile:  {}ms", avg as u64);
            }
            if let Some(avg) = agg.avg_map_ms {
                let formatted = if avg >= 1000.0 {
                    format!("{:.1}s", avg / 1000.0)
                } else {
                    format!("{}ms", avg as u64)
                };
                println!("    avg map:      {}", formatted);
            }
            println!("    errors:       {}", agg.total_errors);
            println!("    warnings:     {}", agg.total_warnings);

            if !agg.command_counts.is_empty() {
                let mut cmds: Vec<_> = agg.command_counts.iter().collect();
                cmds.sort_by(|a, b| b.1.cmp(a.1));
                let top: Vec<String> = cmds
                    .iter()
                    .take(5)
                    .map(|(k, v)| format!("{} ({})", k, v))
                    .collect();
                println!("    top commands: {}", top.join(", "));
            }

            if !agg.language_percentages.is_empty() {
                let mut langs: Vec<_> = agg.language_percentages.iter().collect();
                langs.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
                let lang_str: Vec<String> = langs
                    .iter()
                    .map(|(k, v)| format!("{} {:.0}%", k, v))
                    .collect();
                println!("    languages:    {}", lang_str.join(", "));
            }
        }
    }

    0
}

fn load_telemetry_aggregate(
    keel_dir: &std::path::Path,
) -> Option<keel_core::telemetry::TelemetryAggregate> {
    let telemetry_path = keel_dir.join("telemetry.db");
    if !telemetry_path.exists() {
        return None;
    }
    let store = keel_core::telemetry::TelemetryStore::open(&telemetry_path).ok()?;
    let agg = store.aggregate(30).ok()?;
    if agg.total_invocations == 0 {
        return None;
    }
    Some(agg)
}
