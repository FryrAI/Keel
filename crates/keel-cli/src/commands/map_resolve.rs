use std::collections::HashMap;
use std::path::Path;

/// Resolve a cross-file call reference by matching imports to the global name index.
pub fn resolve_cross_file_call(
    callee_name: &str,
    imports: &[keel_parsers::resolver::Import],
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
    file_module_ids: &HashMap<String, u64>,
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
            if let Some(candidates) = global_name_index.get(func_name) {
                // Resolve the import to a module file
                if let Some(resolved_module) =
                    resolve_import_to_module(&imp.source, file_module_ids)
                {
                    let resolved_file = file_module_ids
                        .iter()
                        .find(|(_, &id)| id == resolved_module)
                        .map(|(f, _)| f.as_str());
                    if let Some(rf) = resolved_file {
                        for (file, node_id) in candidates {
                            if file == rf {
                                return Some(*node_id);
                            }
                        }
                    }
                }
                // Fallback: match candidates from files in the package directory
                let source = &imp.source;
                let last_seg = source.rsplit('/').next().unwrap_or(source);
                for (file, node_id) in candidates {
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
        let names_match = imp.imported_names.iter().any(|n| n == func_name || n == "*");
        if !names_match {
            continue;
        }
        if let Some(candidates) = global_name_index.get(func_name) {
            let source = &imp.source;
            for (file, node_id) in candidates {
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
                    for (file, node_id) in candidates {
                        if file == rf {
                            return Some(*node_id);
                        }
                    }
                }
            }
            for (file, node_id) in candidates {
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
                let fname = Path::new(p).file_name().and_then(|f| f.to_str()).unwrap_or("");
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
        let rest = import_source.strip_prefix("crate::").unwrap_or(import_source);
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
pub fn resolve_same_directory_call(
    callee_name: &str,
    caller_file: &str,
    global_name_index: &HashMap<String, Vec<(String, u64)>>,
) -> Option<u64> {
    let caller_dir = Path::new(caller_file).parent()?.to_str()?;
    if let Some(candidates) = global_name_index.get(callee_name) {
        for (file, node_id) in candidates {
            if file != caller_file {
                if let Some(dir) = Path::new(file.as_str()).parent().and_then(|p| p.to_str()) {
                    if dir == caller_dir {
                        return Some(*node_id);
                    }
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
        return Some((node_id, 0.70)); // cross-package, lower confidence
    }

    // For qualified calls like "pkg.Func", extract the function part
    let func_name = callee_name
        .rsplit_once('.')
        .or_else(|| callee_name.rsplit_once("::"))
        .map(|(_, f)| f)
        .unwrap_or(callee_name);

    if func_name != callee_name {
        if let Some(&node_id) = pkg_nodes.get(func_name) {
            return Some((node_id, 0.70));
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
    // Bare package name
    if !source.starts_with('.') && !source.starts_with('/') {
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

/// Find which definition contains a given line number.
pub fn find_containing_def(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_module_ids(entries: &[(&str, u64)]) -> HashMap<String, u64> {
        entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn test_resolve_go_import_to_module() {
        let modules = make_module_ids(&[
            ("cmd/root.go", 1),
            ("internal/cobra/command.go", 2),
            ("pkg/utils/helper.go", 3),
        ]);
        let result = resolve_import_to_module("github.com/spf13/cobra", &modules);
        assert_eq!(result, Some(2));

        let result = resolve_import_to_module("github.com/myorg/mylib/utils", &modules);
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_resolve_rust_crate_import_to_module() {
        let modules = make_module_ids(&[
            ("src/store.rs", 10),
            ("src/main.rs", 11),
            ("src/hash/mod.rs", 12),
        ]);
        let result = resolve_import_to_module("crate::store::GraphStore", &modules);
        assert_eq!(result, Some(10));

        let result = resolve_import_to_module("crate::hash", &modules);
        assert_eq!(result, Some(12));
    }

    #[test]
    fn test_resolve_rust_workspace_crate_import() {
        let modules = make_module_ids(&[
            ("crates/keel-core/src/store.rs", 20),
            ("crates/keel-core/src/types.rs", 21),
        ]);
        let result = resolve_import_to_module("crate::store::GraphStore", &modules);
        assert_eq!(result, Some(20));
    }

    #[test]
    fn test_resolve_exact_match() {
        let modules = make_module_ids(&[("src/lib.rs", 5)]);
        let result = resolve_import_to_module("src/lib.rs", &modules);
        assert_eq!(result, Some(5));
    }

    #[test]
    fn test_resolve_relative_ts_import() {
        let modules = make_module_ids(&[("utils.ts", 7), ("components/index.ts", 8)]);
        let result = resolve_import_to_module("./utils", &modules);
        assert_eq!(result, Some(7));

        let result = resolve_import_to_module("./components", &modules);
        assert_eq!(result, Some(8));
    }

    #[test]
    fn test_resolve_unknown_import_returns_none() {
        let modules = make_module_ids(&[("src/main.rs", 1)]);
        let result = resolve_import_to_module("std::collections::HashMap", &modules);
        assert_eq!(result, None);

        let result = resolve_import_to_module("react", &modules);
        assert_eq!(result, None);
    }
}
