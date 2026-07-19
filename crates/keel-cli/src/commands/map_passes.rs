//! First-pass and second-pass logic for `keel map`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use keel_core::hash::compute_hash;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::walker::WalkEntry;

use super::call_resolve::{resolve_call_reference, CallSiteCtx};
use super::map_lang_resolve::ResolverSet;
use super::map_resolve::{find_containing_def, resolve_import_to_module, CallIndex};
use keel_core::paths::make_relative;

/// Per-file parse data collected during the first pass for use in the second pass.
pub struct FileParseData {
    pub file_path: String,
    /// `detect_language` result, used to pick the Tier-2 resolver in the second pass.
    pub language: String,
    pub definitions: Vec<keel_parsers::resolver::Definition>,
    pub references: Vec<keel_parsers::resolver::Reference>,
    pub imports: Vec<keel_parsers::resolver::Import>,
}

/// First pass: create nodes and same-file edges, collecting parse data for cross-file resolution.
#[allow(clippy::too_many_arguments)]
pub fn first_pass(
    entries: &[WalkEntry],
    cwd: &Path,
    verbose: bool,
    ts: &dyn LanguageResolver,
    py: &dyn LanguageResolver,
    go_resolver: &dyn LanguageResolver,
    rs: &dyn LanguageResolver,
    node_changes: &mut Vec<NodeChange>,
    edge_changes: &mut Vec<EdgeChange>,
    next_id: &mut u64,
    name_to_id: &mut HashMap<(String, String), u64>,
    global_name_index: &mut HashMap<String, Vec<(String, u64)>>,
    file_module_ids: &mut HashMap<String, u64>,
    assigned_hashes: &mut HashSet<String>,
    valid_node_ids: &mut HashSet<u64>,
    body_index: &mut Vec<keel_core::types::BodyIndexEntry>,
) -> Vec<FileParseData> {
    let mut all_file_data: Vec<FileParseData> = Vec::new();

    for entry in entries {
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
            l if keel_parsers::treesitter::is_typescript_family(l) => ts,
            "python" => py,
            "go" => go_resolver,
            "rust" => rs,
            _ => continue,
        };

        let result = resolver.parse_file(&entry.path, &content);
        let file_path = make_relative(cwd, &entry.path);

        // Create module node for this file
        let module_id = *next_id;
        *next_id += 1;
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
            // Canonical content hash (single source of truth: `Definition::hash`),
            // salting with the file path only on a collision, exactly as the
            // rest of the pipeline does.
            let mut hash = def.hash();
            if assigned_hashes.contains(&hash) {
                hash = def.hash_disambiguated(&file_path);
            }
            assigned_hashes.insert(hash.clone());
            let node_id = *next_id;
            *next_id += 1;
            valid_node_ids.insert(node_id);

            name_to_id.insert((file_path.clone(), def.name.clone()), node_id);
            global_name_index
                .entry(def.name.clone())
                .or_default()
                .push((file_path.clone(), node_id));

            // Feed the W006 duplicate-implementation index. Trivial bodies
            // are skipped here only to bound index size; enforcement applies
            // its own (higher, compile-time-asserted) similarity threshold.
            if def.kind == NodeKind::Function {
                let normalized = keel_core::hash::normalize_body(&def.body_text);
                if normalized.len() >= keel_core::hash::MIN_INDEXED_BODY_LEN {
                    body_index.push(keel_core::types::BodyIndexEntry {
                        body_hash: keel_core::hash::hash_normalized_body(&normalized),
                        node_hash: hash.clone(),
                        name: def.name.clone(),
                        file_path: file_path.clone(),
                        line: def.line_start,
                    });
                }
            }

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
            let edge_id = *next_id;
            *next_id += 1;
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
                if let Some(&target_id) =
                    name_to_id.get(&(file_path.clone(), reference.name.clone()))
                {
                    let source_id = find_containing_def(
                        &result.definitions,
                        reference.line,
                        &file_path,
                        name_to_id,
                        Some(module_id),
                    );
                    if let Some(src_id) = source_id {
                        if src_id != target_id {
                            let edge_id = *next_id;
                            *next_id += 1;
                            edge_changes.push(EdgeChange::Add(GraphEdge {
                                id: edge_id,
                                source_id: src_id,
                                target_id,
                                kind: EdgeKind::Calls,
                                file_path: file_path.clone(),
                                line: reference.line,
                                confidence: keel_core::confidence::SAME_FILE_CALL,
                            }));
                        }
                    }
                }
            }
        }

        all_file_data.push(FileParseData {
            file_path,
            language: entry.language.clone(),
            definitions: result.definitions,
            references: result.references,
            imports: result.imports,
        });
    }

    all_file_data
}

/// Second pass: cross-file call edges and import edges.
///
/// `baml_fn_index` maps BAML boundary function names to their node ids; calls
/// left unresolved by the normal pipeline are matched against it so calls into
/// `.baml` functions resolve to the boundary surface. It is empty (and thus a
/// no-op) for repos that use no BAML.
#[allow(clippy::too_many_arguments)]
pub fn second_pass(
    all_file_data: &[FileParseData],
    cwd: &Path,
    resolvers: &ResolverSet,
    name_to_id: &HashMap<(String, String), u64>,
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
    file_module_ids: &HashMap<String, u64>,
    package_node_index: &HashMap<String, HashMap<String, u64>>,
    baml_fn_index: &HashMap<String, u64>,
    edge_changes: &mut Vec<EdgeChange>,
    next_id: &mut u64,
    node_tiers: &mut HashMap<u64, (String, f64)>,
) {
    // The map backs the shared call-resolution ladder with its in-memory
    // indices (built during the first pass); the compile sync backs the same
    // ladder with graph-backed lookups. See `super::call_resolve`.
    let idx = MapIndex {
        global_name_index,
        file_module_ids,
        package_node_index,
        baml_fn_index,
        name_to_id,
    };

    for file_data in all_file_data {
        let file_path = &file_data.file_path;
        // Absolute path the resolver cached this file under (see `call_site_for`).
        let abs_file = cwd.join(file_path);

        // Create Imports edges between modules
        if let Some(&src_module_id) = file_module_ids.get(file_path.as_str()) {
            for imp in &file_data.imports {
                let tgt_module_id = resolve_import_to_module(&imp.source, file_module_ids);
                if let Some(tgt_id) = tgt_module_id {
                    if tgt_id != src_module_id {
                        let edge_id = *next_id;
                        *next_id += 1;
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

        let ctx = CallSiteCtx {
            resolvers,
            language: &file_data.language,
            file_path,
            abs_file: &abs_file,
            imports: &file_data.imports,
            definitions: &file_data.definitions,
        };

        // Resolve cross-file call references (same-file calls were already
        // linked in the first pass, so skip references to a same-file def).
        for reference in &file_data.references {
            if reference.kind != keel_parsers::resolver::ReferenceKind::Call {
                continue;
            }
            if name_to_id.contains_key(&(file_path.clone(), reference.name.clone())) {
                continue;
            }

            let Some(resolved) = resolve_call_reference(&idx, &ctx, reference) else {
                continue;
            };

            let source_id = find_containing_def(
                &file_data.definitions,
                reference.line,
                file_path,
                name_to_id,
                file_module_ids.get(file_path.as_str()).copied(),
            );
            if let Some(src_id) = source_id {
                if src_id != resolved.target_id {
                    let edge_id = *next_id;
                    *next_id += 1;
                    edge_changes.push(EdgeChange::Add(GraphEdge {
                        id: edge_id,
                        source_id: src_id,
                        target_id: resolved.target_id,
                        kind: EdgeKind::Calls,
                        file_path: file_path.clone(),
                        line: reference.line,
                        confidence: resolved.confidence,
                    }));
                    // Record which tier resolved this caller's edges, keeping
                    // the highest-confidence tier seen for the source node.
                    let entry = node_tiers
                        .entry(src_id)
                        .or_insert_with(|| (resolved.tier.clone(), resolved.confidence));
                    if resolved.confidence >= entry.1 {
                        *entry = (resolved.tier.clone(), resolved.confidence);
                    }
                }
            }
        }
    }
}

/// [`CallIndex`] backed by the map's in-memory indices — the fast path where
/// every candidate is already in RAM from the first pass.
struct MapIndex<'a> {
    global_name_index: &'a HashMap<String, Vec<(String, u64)>>,
    file_module_ids: &'a HashMap<String, u64>,
    package_node_index: &'a HashMap<String, HashMap<String, u64>>,
    baml_fn_index: &'a HashMap<String, u64>,
    name_to_id: &'a HashMap<(String, String), u64>,
}

impl CallIndex for MapIndex<'_> {
    fn candidates(&self, name: &str) -> Cow<'_, [(String, u64)]> {
        // Borrowed: the map's index already owns these lists.
        self.global_name_index
            .get(name)
            .map(|v| Cow::Borrowed(v.as_slice()))
            .unwrap_or(Cow::Borrowed(&[]))
    }
    fn module_files(&self) -> &HashMap<String, u64> {
        self.file_module_ids
    }
    fn name_to_id(&self) -> &HashMap<(String, String), u64> {
        self.name_to_id
    }
    fn package_index(&self) -> &HashMap<String, HashMap<String, u64>> {
        self.package_node_index
    }
    fn baml_index(&self) -> &HashMap<String, u64> {
        self.baml_fn_index
    }
}
