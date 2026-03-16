use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeKind};
use keel_output::OutputFormatter;

/// Run `keel skeleton <file>` — signature-only view of a file.
///
/// Shows function/method signatures, class/struct/interface definitions,
/// type aliases, and imports — without implementation bodies. Dramatically
/// more token-efficient than reading the full file.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    file: String,
    docs: bool,
    private: bool,
    json: bool,
    llm: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel skeleton: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel skeleton: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel skeleton: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Normalize to relative path
    let path = std::path::Path::new(&file);
    let rel_path = if path.is_absolute() {
        path.strip_prefix(&cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        file.clone()
    };

    let nodes = store.get_nodes_in_file(&rel_path);
    if nodes.is_empty() {
        eprintln!("keel skeleton: no data for file: {}", rel_path);
        eprintln!("hint: Run `keel map` first to populate the graph.");
        return 2;
    }

    let filtered = filter_nodes(&nodes, private);

    if verbose {
        let total_lines: u32 = nodes
            .iter()
            .filter(|n| n.kind != NodeKind::Module)
            .map(|n| n.line_end.saturating_sub(n.line_start) + 1)
            .sum();
        let skel_lines = count_skeleton_lines(&filtered, docs);
        eprintln!(
            "keel skeleton: {} — {} symbols, ~{} lines -> ~{} lines",
            rel_path,
            filtered.len(),
            total_lines,
            skel_lines,
        );
    }

    if json {
        print_json(&rel_path, &filtered, docs);
    } else {
        print_text(&rel_path, &filtered, docs, llm);
    }

    0
}

/// Filter nodes: skip Module, skip private unless `--private`.
fn filter_nodes(nodes: &[GraphNode], include_private: bool) -> Vec<&GraphNode> {
    nodes
        .iter()
        .filter(|n| n.kind != NodeKind::Module)
        .filter(|n| include_private || n.is_public)
        .collect()
}

/// Estimate skeleton line count (signatures + optional docstrings).
fn count_skeleton_lines(nodes: &[&GraphNode], docs: bool) -> usize {
    let mut count = 0;
    for node in nodes {
        count += 1; // signature line
        if docs && node.docstring.is_some() {
            // Rough estimate: 1 line for short docstrings, more for multi-line
            let ds = node.docstring.as_deref().unwrap_or("");
            count += ds.lines().count().max(1);
        }
    }
    count
}

/// Detect language from file extension for formatting.
fn detect_language(file_path: &str) -> &str {
    if let Some(ext) = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
    {
        match ext {
            "py" => "python",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "go" => "go",
            "rs" => "rust",
            _ => "unknown",
        }
    } else {
        "unknown"
    }
}

/// Format a signature with `...` body placeholder, language-aware.
fn format_skeleton_line(node: &GraphNode, lang: &str, indent: &str) -> String {
    let sig = if node.signature.is_empty() {
        &node.name
    } else {
        &node.signature
    };

    match lang {
        "python" => format!("{}{}  ...", indent, sig),
        "rust" => format!("{}{} {{ ... }}", indent, sig),
        "go" => format!("{}{} {{ ... }}", indent, sig),
        "typescript" | "javascript" => format!("{}{} {{ ... }}", indent, sig),
        _ => format!("{}{}  ...", indent, sig),
    }
}

fn print_json(file_path: &str, nodes: &[&GraphNode], docs: bool) {
    let symbols: Vec<serde_json::Value> = nodes
        .iter()
        .map(|node| {
            let mut obj = serde_json::json!({
                "name": node.name,
                "hash": node.hash,
                "kind": node.kind.as_str(),
                "line_start": node.line_start,
                "line_end": node.line_end,
                "is_public": node.is_public,
                "signature": node.signature,
            });
            if docs {
                if let Some(ref ds) = node.docstring {
                    obj["docstring"] = serde_json::Value::String(ds.clone());
                }
            }
            obj
        })
        .collect();

    let func_count = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Function)
        .count();
    let class_count = nodes.iter().filter(|n| n.kind == NodeKind::Class).count();

    println!(
        "{}",
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "command": "skeleton",
            "file": file_path,
            "summary": {
                "functions": func_count,
                "classes": class_count,
                "total": nodes.len(),
            },
            "symbols": symbols,
        })
    );
}

/// Grouped data for text output rendering.
struct TextData<'a> {
    file_path: &'a str,
    nodes: &'a [&'a GraphNode],
    docs: bool,
    lang: &'a str,
    class_ids: Vec<u64>,
    classes: Vec<&'a &'a GraphNode>,
    functions: Vec<&'a &'a GraphNode>,
    func_count: usize,
    class_count: usize,
}

impl<'a> TextData<'a> {
    fn new(file_path: &'a str, nodes: &'a [&'a GraphNode], docs: bool) -> Self {
        let lang = detect_language(file_path);
        let func_count = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .count();
        let class_count = nodes.iter().filter(|n| n.kind == NodeKind::Class).count();
        let classes: Vec<&&GraphNode> =
            nodes.iter().filter(|n| n.kind == NodeKind::Class).collect();
        let functions: Vec<&&GraphNode> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function && n.module_id == 0)
            .collect();
        let class_ids: Vec<u64> = classes.iter().map(|c| c.id).collect();
        Self {
            file_path,
            nodes,
            docs,
            lang,
            class_ids,
            classes,
            functions,
            func_count,
            class_count,
        }
    }
}

fn print_text(file_path: &str, nodes: &[&GraphNode], docs: bool, llm: bool) {
    let data = TextData::new(file_path, nodes, docs);
    if llm {
        print_llm(&data);
    } else {
        print_human(&data);
    }
}

fn print_human(data: &TextData<'_>) {
    let skel_lines = count_skeleton_lines(data.nodes, data.docs);
    let total_lines: u32 = data
        .nodes
        .iter()
        .map(|n| n.line_end.saturating_sub(n.line_start) + 1)
        .sum();

    println!(
        "SKELETON {} ({} functions, {} classes, {} lines -> {} lines)\n",
        data.file_path, data.func_count, data.class_count, total_lines, skel_lines,
    );

    // Print classes with their methods
    for class in &data.classes {
        println!("  {}:", class.name);
        if data.docs {
            if let Some(ref ds) = class.docstring {
                for line in ds.lines() {
                    println!("    # {}", line);
                }
            }
        }
        // Find methods belonging to this class
        let methods: Vec<&&GraphNode> = data
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function && n.module_id == class.id)
            .collect();
        for method in &methods {
            if data.docs {
                if let Some(ref ds) = method.docstring {
                    for line in ds.lines() {
                        println!("      # {}", line);
                    }
                }
            }
            println!("{}", format_skeleton_line(method, data.lang, "    "));
        }
        println!();
    }

    // Print top-level functions (not methods)
    for func in &data.functions {
        if data.class_ids.contains(&func.module_id) {
            continue; // already printed as method
        }
        if data.docs {
            if let Some(ref ds) = func.docstring {
                for line in ds.lines() {
                    println!("    # {}", line);
                }
            }
        }
        println!("{}", format_skeleton_line(func, data.lang, "  "));
    }
}

fn print_llm(data: &TextData<'_>) {
    // LLM-compact format: hash-prefixed, minimal whitespace
    println!("SKEL {}", data.file_path);
    for class in &data.classes {
        println!("  [{}] {} {}", class.hash, class.kind.as_str(), class.name);
        if data.docs {
            if let Some(ref ds) = class.docstring {
                let first_line = ds.lines().next().unwrap_or("");
                println!("    # {}", first_line);
            }
        }
        let methods: Vec<&&GraphNode> = data
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function && n.module_id == class.id)
            .collect();
        for method in &methods {
            print!("    [{}] ", method.hash);
            println!("{}", format_skeleton_line(method, data.lang, ""));
        }
    }
    // Top-level functions
    for node in data.nodes {
        if node.kind == NodeKind::Function && !data.class_ids.contains(&node.module_id) {
            print!("  [{}] ", node.hash);
            println!("{}", format_skeleton_line(node, data.lang, ""));
        }
    }
}
