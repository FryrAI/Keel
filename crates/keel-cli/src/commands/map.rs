use std::collections::HashMap;
use std::fs;
use std::path::Path;

use keel_core::hash::compute_hash;
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

    let mut node_changes = Vec::new();
    let mut edge_changes = Vec::new();
    let mut next_id = 1u64;
    // Map (file_path, name) -> node_id for building edges
    let mut name_to_id: HashMap<(String, String), u64> = HashMap::new();

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
            let hash = compute_hash(
                &def.signature,
                &def.body_text,
                def.docstring.as_deref().unwrap_or(""),
            );
            let node_id = next_id;
            next_id += 1;

            name_to_id.insert((file_path.clone(), def.name.clone()), node_id);

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

        // Create call edges from references
        for reference in &result.references {
            if reference.kind == keel_parsers::resolver::ReferenceKind::Call {
                // Try to find the target in the same file
                if let Some(&target_id) =
                    name_to_id.get(&(file_path.clone(), reference.name.clone()))
                {
                    // Find the source node (function containing this call)
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
    }

    // Apply node changes
    if let Err(e) = store.update_nodes(node_changes) {
        eprintln!("keel map: failed to update nodes: {}", e);
        return 2;
    }

    // Apply edge changes
    if let Err(e) = store.update_edges(edge_changes) {
        eprintln!("keel map: failed to update edges: {}", e);
        return 2;
    }

    if verbose {
        eprintln!("keel map: mapped {} files", entries.len());
    }

    let _ = formatter;
    0
}

/// Make a path relative to the project root.
fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
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
