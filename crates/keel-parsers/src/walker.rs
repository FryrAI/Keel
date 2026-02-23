use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::monorepo::MonorepoLayout;
use crate::treesitter::detect_language;

pub struct WalkEntry {
    pub path: PathBuf,
    pub language: String,
    pub package: Option<String>,
}

pub struct FileWalker {
    root: PathBuf,
}

impl FileWalker {
    /// Creates a new file walker rooted at the given directory.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// Walks the root directory and returns all recognized source files, respecting gitignore and `.keelignore`.
    pub fn walk(&self) -> Vec<WalkEntry> {
        let mut entries = Vec::new();

        let walker = WalkBuilder::new(&self.root)
            .hidden(true)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .add_custom_ignore_filename(".keelignore")
            .build();

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.into_path();
            if let Some(lang) = detect_language(&path) {
                entries.push(WalkEntry {
                    path,
                    language: lang.to_string(),
                    package: None,
                });
            }
        }

        entries
    }

    /// Walks files and annotates each with its monorepo package using longest-prefix match.
    pub fn walk_with_packages(&self, layout: &MonorepoLayout) -> Vec<WalkEntry> {
        let mut entries = self.walk();
        for entry in &mut entries {
            entry.package = find_package_for_path(&entry.path, layout);
        }
        entries
    }
}

/// Find which package a file belongs to using longest-prefix match.
fn find_package_for_path(file_path: &Path, layout: &MonorepoLayout) -> Option<String> {
    let mut best_match: Option<&str> = None;
    let mut best_len = 0;

    for pkg in &layout.packages {
        if file_path.starts_with(&pkg.path) {
            let pkg_len = pkg.path.as_os_str().len();
            if pkg_len > best_len {
                best_len = pkg_len;
                best_match = Some(&pkg.name);
            }
        }
    }

    best_match.map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monorepo::{MonorepoKind, PackageInfo};
    use std::fs;

    #[test]
    fn test_walker_finds_source_files() {
        let dir = std::env::temp_dir().join("keel_walker_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("src/lib.py"), "def f(): pass").unwrap();
        fs::write(dir.join("README.md"), "# Hello").unwrap();

        let walker = FileWalker::new(&dir);
        let entries = walker.walk();

        assert_eq!(entries.len(), 2);
        let langs: Vec<_> = entries.iter().map(|e| e.language.as_str()).collect();
        assert!(langs.contains(&"rust"));
        assert!(langs.contains(&"python"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_walker_respects_keelignore() {
        let dir = std::env::temp_dir().join("keel_walker_ignore_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("vendor")).unwrap();
        fs::write(dir.join("src/app.ts"), "export {}").unwrap();
        fs::write(dir.join("vendor/lib.ts"), "export {}").unwrap();
        fs::write(dir.join(".keelignore"), "vendor/\n").unwrap();

        let walker = FileWalker::new(&dir);
        let entries = walker.walk();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.to_str().unwrap().contains("app.ts"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_walk_with_packages_annotates_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create package dirs with source files
        fs::create_dir_all(root.join("packages/web/src")).unwrap();
        fs::create_dir_all(root.join("packages/api/src")).unwrap();
        fs::write(root.join("packages/web/src/app.ts"), "export {}").unwrap();
        fs::write(root.join("packages/api/src/main.ts"), "export {}").unwrap();
        fs::write(root.join("root.ts"), "export {}").unwrap();

        let layout = MonorepoLayout {
            kind: MonorepoKind::NpmWorkspaces,
            packages: vec![
                PackageInfo {
                    name: "web".to_string(),
                    path: root.join("packages/web"),
                    kind: MonorepoKind::NpmWorkspaces,
                    language: "typescript".to_string(),
                },
                PackageInfo {
                    name: "api".to_string(),
                    path: root.join("packages/api"),
                    kind: MonorepoKind::NpmWorkspaces,
                    language: "typescript".to_string(),
                },
            ],
        };

        let walker = FileWalker::new(root);
        let entries = walker.walk_with_packages(&layout);

        // Find the web and api entries
        let web_entry = entries
            .iter()
            .find(|e| e.path.to_str().unwrap().contains("packages/web"));
        let api_entry = entries
            .iter()
            .find(|e| e.path.to_str().unwrap().contains("packages/api"));
        let root_entry = entries
            .iter()
            .find(|e| e.path.file_name().and_then(|n| n.to_str()) == Some("root.ts"));

        assert_eq!(web_entry.unwrap().package.as_deref(), Some("web"));
        assert_eq!(api_entry.unwrap().package.as_deref(), Some("api"));
        assert_eq!(root_entry.unwrap().package, None);
    }
}
