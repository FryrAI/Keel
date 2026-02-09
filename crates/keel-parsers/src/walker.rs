use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::treesitter::detect_language;

pub struct WalkEntry {
    pub path: PathBuf,
    pub language: String,
}

pub struct FileWalker {
    root: PathBuf,
}

impl FileWalker {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

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

            if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                continue;
            }

            let path = entry.into_path();
            if let Some(lang) = detect_language(&path) {
                entries.push(WalkEntry {
                    path,
                    language: lang.to_string(),
                });
            }
        }

        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
