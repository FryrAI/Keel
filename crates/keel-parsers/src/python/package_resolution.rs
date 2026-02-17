//! Resolve dotted Python module paths through filesystem package chains.
//!
//! Handles:
//! - Regular packages with `__init__.py`
//! - Namespace packages (PEP 420) — directories without `__init__.py`
//! - Deep nested chains like `a.b.c.d`

use std::path::{Path, PathBuf};

/// Resolve a dotted module path (e.g. `a.b.c`) through the filesystem.
///
/// Walks each segment as a directory, then resolves the final segment
/// as either a `.py` file or a package (`__init__.py`). Supports
/// namespace packages (directories without `__init__.py`).
///
/// Returns `None` if any intermediate directory does not exist.
pub fn resolve_python_package_chain(
    base_dir: &Path,
    module_path: &str,
) -> Option<PathBuf> {
    let parts: Vec<&str> = module_path.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let mut current = base_dir.to_path_buf();

    // Walk intermediate segments — each must be a directory
    for part in &parts[..parts.len().saturating_sub(1)] {
        let pkg_dir = current.join(part);
        if pkg_dir.is_dir() {
            current = pkg_dir;
        } else {
            return None;
        }
    }

    // Resolve the final segment
    let last = parts.last()?;
    let as_file = current.join(format!("{last}.py"));
    let as_pkg = current.join(last).join("__init__.py");

    if as_file.exists() {
        Some(as_file)
    } else if as_pkg.exists() {
        Some(as_pkg)
    } else if current.join(last).is_dir() {
        // Namespace package: directory exists but no __init__.py
        // Return the __init__.py path as a convention even though
        // the file doesn't exist — callers know to check is_dir()
        Some(current.join(last).join("__init__.py"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keel_py_pkg_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_simple_module_file() {
        let dir = setup_dir("simple_mod");
        fs::write(dir.join("utils.py"), "def helper(): pass").unwrap();

        let result = resolve_python_package_chain(&dir, "utils");
        assert_eq!(result, Some(dir.join("utils.py")));

        cleanup(&dir);
    }

    #[test]
    fn test_package_with_init() {
        let dir = setup_dir("pkg_init");
        fs::create_dir_all(dir.join("mypackage")).unwrap();
        fs::write(dir.join("mypackage/__init__.py"), "").unwrap();

        let result = resolve_python_package_chain(&dir, "mypackage");
        assert_eq!(result, Some(dir.join("mypackage/__init__.py")));

        cleanup(&dir);
    }

    #[test]
    fn test_nested_package() {
        let dir = setup_dir("nested");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/__init__.py"), "").unwrap();
        fs::write(dir.join("a/b/__init__.py"), "").unwrap();
        fs::write(dir.join("a/b/mod.py"), "").unwrap();

        let result = resolve_python_package_chain(&dir, "a.b.mod");
        assert_eq!(result, Some(dir.join("a/b/mod.py")));

        cleanup(&dir);
    }

    #[test]
    fn test_namespace_package_no_init() {
        let dir = setup_dir("ns_pkg");
        fs::create_dir_all(dir.join("ns_pkg")).unwrap();
        fs::write(dir.join("ns_pkg/module.py"), "").unwrap();
        // No __init__.py in ns_pkg

        let result = resolve_python_package_chain(&dir, "ns_pkg.module");
        assert_eq!(result, Some(dir.join("ns_pkg/module.py")));

        cleanup(&dir);
    }

    #[test]
    fn test_missing_intermediate_dir() {
        let dir = setup_dir("missing");
        // Don't create any subdirectories

        let result = resolve_python_package_chain(&dir, "a.b.c");
        assert_eq!(result, None);

        cleanup(&dir);
    }

    #[test]
    fn test_prefers_file_over_package() {
        let dir = setup_dir("prefer_file");
        fs::create_dir_all(dir.join("thing")).unwrap();
        fs::write(dir.join("thing.py"), "").unwrap();
        fs::write(dir.join("thing/__init__.py"), "").unwrap();

        // File should be preferred over package
        let result = resolve_python_package_chain(&dir, "thing");
        assert_eq!(result, Some(dir.join("thing.py")));

        cleanup(&dir);
    }

    #[test]
    fn test_deep_chain() {
        let dir = setup_dir("deep");
        fs::create_dir_all(dir.join("a/b/c/d")).unwrap();
        fs::write(dir.join("a/__init__.py"), "").unwrap();
        fs::write(dir.join("a/b/__init__.py"), "").unwrap();
        fs::write(dir.join("a/b/c/__init__.py"), "").unwrap();
        fs::write(dir.join("a/b/c/d/__init__.py"), "").unwrap();

        let result = resolve_python_package_chain(&dir, "a.b.c.d");
        assert_eq!(result, Some(dir.join("a/b/c/d/__init__.py")));

        cleanup(&dir);
    }
}
