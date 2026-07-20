use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

/// Candidate provider for the shared call-resolution ladder
/// ([`super::call_resolve::resolve_call_reference`]).
///
/// The two pipelines that resolve call edges — `keel map`'s second pass and
/// `keel compile`'s incremental sync — differ only in where candidates come
/// from: the map holds them in the in-memory indices it built during the walk,
/// while the compile sync looks them up in the persisted graph. This trait is
/// that seam, so both run the identical ladder instead of drifting copies.
pub trait CallIndex {
    /// Definitions carrying this bare name: `(relative_file_path, node_id)`.
    ///
    /// Returns a [`Cow`] because the ladder asks for candidates up to three
    /// times per unresolved reference and the two implementations pay very
    /// different costs: the map holds its candidate lists in RAM and lends them
    /// out for free, while the graph-backed index has to query SQLite. Handing
    /// back an owned `Vec` forced the map to clone on every call for nothing.
    fn candidates(&self, name: &str) -> Cow<'_, [(String, u64)]>;
    /// Every known `relative_file_path -> module_id` mapping, for matching an
    /// import specifier to the module it resolves to.
    fn module_files(&self) -> &HashMap<String, u64>;
    /// `(file_path, name) -> node id` for definitions the pipeline knows
    /// locally, used to resolve same-file method calls.
    fn name_to_id(&self) -> &HashMap<(String, String), u64>;
    /// `package -> (symbol -> node id)` for monorepo cross-package resolution.
    fn package_index(&self) -> &HashMap<String, HashMap<String, u64>>;
    /// Boundary function name -> `(node id, confidence)` (e.g. BAML `.baml`
    /// functions). The confidence is the one reported by the provider that
    /// produced the entry, carried per entry so multiple providers can coexist
    /// without a shared scalar collapsing every boundary edge to the last
    /// provider's tier.
    fn boundary_index(&self) -> &HashMap<String, (u64, f64)>;
}

/// Resolve a cross-file call reference by matching imports to the candidate index.
pub fn resolve_cross_file_call(
    callee_name: &str,
    imports: &[keel_parsers::resolver::Import],
    idx: &dyn CallIndex,
) -> Option<u64> {
    // Handle qualified calls: "fmt.Println" (Go) or "module::func" (Rust)
    let (qualifier, func_name) = if let Some(dot_pos) = callee_name.find('.') {
        // Go-style: pkg.Func
        (Some(&callee_name[..dot_pos]), &callee_name[dot_pos + 1..])
    } else if let Some(sep_pos) = callee_name.rfind("::") {
        // Rust-style: module::func
        (Some(&callee_name[..sep_pos]), &callee_name[sep_pos + 2..])
    } else {
        (None, callee_name)
    };

    // Candidates for the callee's bare name, and the module map, once.
    let candidates = idx.candidates(func_name);
    let file_module_ids = idx.module_files();

    // For qualified calls, find the import matching the qualifier
    if let Some(qual) = qualifier {
        for imp in imports {
            let qualifier_matches = imp.imported_names.iter().any(|n| n == qual)
                || imp.source.ends_with(&format!("/{qual}"))
                || imp.source.ends_with(&format!("::{qual}"))
                || imp.source == qual;

            if !qualifier_matches {
                continue;
            }

            // Look for the function name in the imported module
            if !candidates.is_empty() {
                // Resolve the import to a module file
                if let Some(resolved_module) =
                    resolve_import_to_module(&imp.source, file_module_ids)
                {
                    let resolved_file = file_module_ids
                        .iter()
                        .find(|(_, &id)| id == resolved_module)
                        .map(|(f, _)| f.as_str());
                    if let Some(rf) = resolved_file {
                        for (file, node_id) in candidates.iter() {
                            if file == rf {
                                return Some(*node_id);
                            }
                        }
                    }
                }
                // Fallback: match candidates from files in the package directory
                let source = &imp.source;
                let last_seg = source.rsplit('/').next().unwrap_or(source);
                for (file, node_id) in candidates.iter() {
                    if let Some(parent) = Path::new(file.as_str()).parent() {
                        if parent.file_name().and_then(|n| n.to_str()) == Some(last_seg) {
                            return Some(*node_id);
                        }
                    }
                }
            }
        }
    }

    // Unqualified calls: check if any import brings this name into scope
    for imp in imports {
        let names_match = imp
            .imported_names
            .iter()
            .any(|n| n == func_name || n == "*");
        if !names_match {
            continue;
        }
        if !candidates.is_empty() {
            let source = &imp.source;
            for (file, node_id) in candidates.iter() {
                if file == source {
                    return Some(*node_id);
                }
            }
            if let Some(resolved_module) = resolve_import_to_module(source, file_module_ids) {
                let resolved_file = file_module_ids
                    .iter()
                    .find(|(_, &id)| id == resolved_module)
                    .map(|(f, _)| f.as_str());
                if let Some(rf) = resolved_file {
                    for (file, node_id) in candidates.iter() {
                        if file == rf {
                            return Some(*node_id);
                        }
                    }
                }
            }
            for (file, node_id) in candidates.iter() {
                if file_module_ids.contains_key(file.as_str()) && source.contains(file.as_str()) {
                    return Some(*node_id);
                }
            }
            if candidates.len() == 1 {
                return Some(candidates[0].1);
            }
        }
    }
    None
}

/// Map a language-specific import source to a file module ID.
///
/// Handles:
/// - Go: "github.com/spf13/cobra" -> matches files in directories ending with "cobra"
/// - Rust: "crate::store::GraphStore" -> matches "src/store.rs" or "src/store/mod.rs"
/// - Rust: "super::foo" -> already resolved by the Rust resolver
/// - TS/Python: relative paths like "./utils" -> matches "utils.ts", "utils/index.ts", etc.
pub fn resolve_import_to_module(
    import_source: &str,
    file_module_ids: &HashMap<String, u64>,
) -> Option<u64> {
    // 1. Exact match (already works for resolved relative imports)
    if let Some(&id) = file_module_ids.get(import_source) {
        return Some(id);
    }

    // 1b. If import_source is an absolute or longer path, try suffix matching
    // This handles resolved crate:: paths that became absolute
    if import_source.contains('/') {
        for (module_path, &id) in file_module_ids {
            if import_source.ends_with(module_path.as_str()) {
                return Some(id);
            }
        }
    }

    // 2. Go package paths: extract last segment and match directory name
    if import_source.contains('/')
        && !import_source.starts_with('.')
        && !import_source.contains("::")
    {
        let last_segment = import_source.rsplit('/').next()?;
        let mut candidates: Vec<(&str, u64)> = file_module_ids
            .iter()
            .filter(|(path, _)| {
                if let Some(parent) = Path::new(path.as_str()).parent() {
                    if let Some(dir_name) = parent.file_name() {
                        return dir_name.to_str() == Some(last_segment);
                    }
                }
                false
            })
            .map(|(p, &id)| (p.as_str(), id))
            .collect();

        if candidates.len() == 1 {
            return Some(candidates[0].1);
        }
        if !candidates.is_empty() {
            candidates.sort_by_key(|(p, _)| {
                let fname = Path::new(p)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("");
                if fname == "mod.go" || fname == "main.go" || fname.contains(last_segment) {
                    0
                } else {
                    1
                }
            });
            return Some(candidates[0].1);
        }
    }

    // 3. Rust crate:: paths: convert to src/ file paths
    if import_source.starts_with("crate::") {
        let rest = import_source
            .strip_prefix("crate::")
            .unwrap_or(import_source);
        let segments: Vec<&str> = rest.split("::").collect();
        for depth in (1..=segments.len()).rev() {
            let module_path = segments[..depth].join("/");
            let suffixes = [
                format!("src/{module_path}.rs"),
                format!("src/{module_path}/mod.rs"),
                format!("{module_path}.rs"),
                format!("{module_path}/mod.rs"),
            ];
            for suffix in &suffixes {
                if let Some(&id) = file_module_ids.get(suffix.as_str()) {
                    return Some(id);
                }
                for (path, &id) in file_module_ids.iter() {
                    if path.ends_with(suffix.as_str()) {
                        return Some(id);
                    }
                }
            }
        }
    }

    // 4. Rust super:: paths (already resolved by resolver, but try matching anyway)
    if import_source.starts_with("super::") {
        if let Some(&id) = file_module_ids.get(import_source) {
            return Some(id);
        }
    }

    // 5. Relative path imports (TS/Python): strip leading ./ and try extensions
    if import_source.starts_with("./") || import_source.starts_with("../") {
        let candidates = [
            format!("{import_source}.ts"),
            format!("{import_source}.tsx"),
            format!("{import_source}.js"),
            format!("{import_source}.py"),
            format!("{import_source}/index.ts"),
            format!("{import_source}/index.tsx"),
            format!("{import_source}/index.js"),
            format!("{import_source}/__init__.py"),
        ];
        for candidate in &candidates {
            let normalized = candidate.strip_prefix("./").unwrap_or(candidate);
            if let Some(&id) = file_module_ids.get(normalized) {
                return Some(id);
            }
        }
    }

    None
}

/// Resolve an unqualified cross-file call by looking in the same directory.
/// In Go, all files in the same directory share a package namespace.
pub fn resolve_same_directory_call(candidates: &[(String, u64)], caller_file: &str) -> Option<u64> {
    let caller_dir = Path::new(caller_file).parent()?.to_str()?;
    for (file, node_id) in candidates {
        if file != caller_file {
            if let Some(dir) = Path::new(file.as_str()).parent().and_then(|p| p.to_str()) {
                if dir == caller_dir {
                    return Some(*node_id);
                }
            }
        }
    }
    None
}

/// Resolve a cross-package import by matching a package name to nodes in that package.
///
/// Handles:
/// - npm `@scope/name` or bare package names
/// - Cargo `crate_name::path`
/// - Go full module paths
///
/// Returns the target node ID if found, along with lower confidence (0.70).
pub fn resolve_package_import(
    callee_name: &str,
    import_source: &str,
    package_node_index: &HashMap<String, HashMap<String, u64>>,
) -> Option<(u64, f64)> {
    // Extract the package name from the import source
    let pkg_name = extract_package_name(import_source)?;

    // Look up the package in the index
    let pkg_nodes = package_node_index.get(&pkg_name)?;

    // Try to find the callee in this package's exported symbols
    if let Some(&node_id) = pkg_nodes.get(callee_name) {
        return Some((node_id, keel_core::confidence::CROSS_PACKAGE));
    }

    // For qualified calls like "pkg.Func", extract the function part
    let func_name = callee_name
        .rsplit_once('.')
        .or_else(|| callee_name.rsplit_once("::"))
        .map(|(_, f)| f)
        .unwrap_or(callee_name);

    if func_name != callee_name {
        if let Some(&node_id) = pkg_nodes.get(func_name) {
            return Some((node_id, keel_core::confidence::CROSS_PACKAGE));
        }
    }

    None
}

/// Extract a package name from an import source string.
fn extract_package_name(source: &str) -> Option<String> {
    // npm scoped packages: @scope/name -> name
    if source.starts_with('@') {
        let parts: Vec<&str> = source.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return Some(parts[1].to_string());
        }
    }
    // Cargo crate imports: crate_name::path -> crate_name
    if source.contains("::") {
        let crate_name = source.split("::").next()?;
        if crate_name != "crate" && crate_name != "super" && crate_name != "self" {
            return Some(crate_name.replace('-', "_"));
        }
    }
    // Go module paths: github.com/org/repo/pkg -> pkg (last segment)
    if source.contains('/') && !source.starts_with('.') {
        return source.rsplit('/').next().map(String::from);
    }
    // Bare package name (no path separators, no Rust-style ::)
    if !source.starts_with('.')
        && !source.starts_with('/')
        && !source.contains("::")
        && !source.contains('/')
    {
        return Some(source.to_string());
    }
    None
}

/// Build a package-level node index: package_name -> (symbol_name -> node_id).
///
/// Used for cross-package resolution in monorepo mode.
pub fn build_package_node_index(
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
    file_packages: &HashMap<String, String>,
) -> HashMap<String, HashMap<String, u64>> {
    let mut index: HashMap<String, HashMap<String, u64>> = HashMap::new();

    for (name, locations) in global_name_index {
        for (file_path, node_id) in locations {
            if let Some(pkg) = file_packages.get(file_path.as_str()) {
                index
                    .entry(pkg.clone())
                    .or_default()
                    .insert(name.clone(), *node_id);
            }
        }
    }

    index
}

/// Map a language resolver's `ResolvedEdge` target back to a graph node id.
///
/// The per-language `resolve_call_edge` implementations report a `target_file`
/// that may be an oxc-resolved absolute path (TypeScript) or a bare import
/// specifier (Go/Rust), while the map's `global_name_index` is keyed on
/// repo-relative paths. This reconciles the two by matching the target name and
/// a path suffix, falling back to a single unambiguous candidate.
pub fn resolve_edge_to_node(candidates: &[(String, u64)], target_file: &str) -> Option<u64> {
    // 1. Exact (already-relative) match.
    if let Some((_, id)) = candidates.iter().find(|(f, _)| f == target_file) {
        return Some(*id);
    }
    // 2. Suffix match: absolute/resolved path ending in the relative index path
    //    (or vice-versa for shorter specifiers).
    if let Some((_, id)) = candidates
        .iter()
        .find(|(f, _)| target_file.ends_with(f.as_str()) || f.ends_with(target_file))
    {
        return Some(*id);
    }
    // 3. Unambiguous single definition of this name.
    if candidates.len() == 1 {
        return Some(candidates[0].1);
    }
    None
}

/// Find the node a reference on `line` should be attributed to: the innermost
/// (smallest-span) non-module definition enclosing the line, falling back to
/// `module_id` (the file's single path-named module node) when no
/// function/method/class contains the line — i.e. a top-level reference.
///
/// The whole-file module is intentionally never selected as an *enclosing*
/// def: doing so (the old behaviour, driven by a synthetic whole-file module
/// definition) mis-attributed every in-function call to the file itself.
pub fn find_containing_def(
    definitions: &[keel_parsers::resolver::Definition],
    line: u32,
    file_path: &str,
    name_to_id: &HashMap<(String, String), u64>,
    module_id: Option<u64>,
) -> Option<u64> {
    let mut innermost: Option<&keel_parsers::resolver::Definition> = None;
    for def in definitions {
        if def.kind == keel_core::types::NodeKind::Module {
            continue;
        }
        if line < def.line_start || line > def.line_end {
            continue;
        }
        let span = def.line_end.saturating_sub(def.line_start);
        match innermost {
            Some(best) if best.line_end.saturating_sub(best.line_start) <= span => {}
            _ => innermost = Some(def),
        }
    }
    if let Some(def) = innermost {
        if let Some(id) = name_to_id
            .get(&(file_path.to_string(), def.name.clone()))
            .copied()
        {
            return Some(id);
        }
    }
    module_id
}

/// Resolve a dotted method/field call (`self.m()`, `obj.m()`) to a same-file
/// definition by its bare final segment.
///
/// `self`/`this` receivers bind to a same-file method of that name at 0.9
/// confidence. Any other receiver resolves only when exactly one same-file
/// definition carries that name, at 0.7 (warning-tier) — never higher, because
/// the receiver's type is unknown and the match is a heuristic.
pub fn resolve_same_file_method(
    reference_name: &str,
    file_path: &str,
    definitions: &[keel_parsers::resolver::Definition],
    name_to_id: &HashMap<(String, String), u64>,
) -> Option<(u64, f64)> {
    let (receiver, method) = reference_name.rsplit_once('.')?;
    let same_file_matches = definitions
        .iter()
        .filter(|d| d.kind != keel_core::types::NodeKind::Module && d.name == method)
        .count();
    if same_file_matches == 0 {
        return None;
    }
    let confidence = if matches!(receiver, "self" | "this") {
        keel_core::confidence::SELF_RECEIVER_METHOD
    } else if same_file_matches == 1 {
        keel_core::confidence::UNFAMILIAR_RECEIVER_METHOD
    } else {
        return None;
    };
    let id = name_to_id
        .get(&(file_path.to_string(), method.to_string()))
        .copied()?;
    Some((id, confidence))
}

#[cfg(test)]
#[path = "map_resolve_tests.rs"]
mod tests;
