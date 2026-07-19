//! BAML boundary scanner.
//!
//! BAML (<https://boundaryml.com>) repos declare statically-typed LLM functions
//! in `baml_src/*.baml` and call them from Rust/Python/TS via a generated client
//! (`baml_client` / `baml_sdk`, usually gitignored). keel has no tree-sitter
//! grammar for `.baml`, so a call like `b.ExtractResume(...)` would otherwise
//! read as an unresolved/external edge — a systematic blind spot for AI-heavy
//! codebases.
//!
//! This module does a lightweight, dependency-free line scan of `.baml` files,
//! extracting top-level `function` and `class` declarations so the `keel map`
//! pipeline can materialise them as recognizable boundary nodes and resolve
//! calls into them.

use std::path::{Path, PathBuf};

/// A `function` or `class` declared in a `.baml` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BamlSymbol {
    /// Declared name (e.g. `ExtractResume`).
    pub name: String,
    /// Repo-relative path of the declaring `.baml` file (forward slashes).
    pub file_path: String,
    /// 1-based line of the declaration.
    pub line: u32,
    /// Best-effort signature text (declaration line, minus the trailing `{`).
    pub signature: String,
}

/// The BAML surface discovered in a repo.
#[derive(Debug, Clone, Default)]
pub struct BamlBoundary {
    /// `function` declarations across all `.baml` files.
    pub functions: Vec<BamlSymbol>,
    /// `class` declarations across all `.baml` files.
    pub classes: Vec<BamlSymbol>,
    /// True when at least one `baml_src` dir or `.baml` file was found.
    pub baml_src_present: bool,
    /// True when a generated client dir (`baml_client` / `baml_sdk`) exists.
    pub client_generated: bool,
}

impl BamlBoundary {
    /// True when no `function` or `class` declarations were found.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty() && self.classes.is_empty()
    }
}

/// Directories that never contain first-party `.baml` sources and are
/// expensive (or pointless) to descend into.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".next",
    "vendor",
    ".keel",
];

/// Names of generated BAML client directories.
const CLIENT_DIRS: &[&str] = &["baml_client", "baml_sdk"];

/// Maximum recursion depth for the scan (guards against pathological trees).
const MAX_DEPTH: u32 = 8;

/// Scan `root` for a BAML surface: parse every `.baml` file and note whether a
/// generated client directory is present.
///
/// Uses a bounded, plain-filesystem walk (not the gitignore-aware walker) so a
/// gitignored `baml_client` is still detected. Returns an empty boundary when
/// the repo uses no BAML.
pub fn scan(root: &Path) -> BamlBoundary {
    let mut boundary = BamlBoundary::default();
    let mut baml_files: Vec<PathBuf> = Vec::new();
    collect(root, 0, &mut baml_files, &mut boundary);

    // Deterministic file order → stable node ids across runs.
    baml_files.sort();
    for path in &baml_files {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        parse_source(&content, &rel, &mut boundary);
    }
    boundary
}

/// Recursively collect `.baml` files and detect client dirs, skipping heavy
/// directories and bounding recursion depth.
fn collect(dir: &Path, depth: u32, baml_files: &mut Vec<PathBuf>, boundary: &mut BamlBoundary) {
    if depth > MAX_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let raw_name = entry.file_name();
        let name = raw_name.to_string_lossy();
        let path = entry.path();

        if file_type.is_dir() {
            if CLIENT_DIRS.contains(&name.as_ref()) {
                boundary.client_generated = true;
                continue; // generated code — nothing to parse here
            }
            if name == "baml_src" {
                boundary.baml_src_present = true;
            }
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect(&path, depth + 1, baml_files, boundary);
        } else if path.extension().and_then(|e| e.to_str()) == Some("baml") {
            boundary.baml_src_present = true;
            baml_files.push(path);
        }
    }
}

/// Parse the text of a single `.baml` file, appending its `function` and
/// `class` declarations to `boundary`.
///
/// Block strings (`#"… "#`, used for prompts) are tracked so declaration-like
/// text inside a prompt body is not mistaken for a real declaration.
fn parse_source(content: &str, rel_path: &str, boundary: &mut BamlBoundary) {
    let mut in_block_string = false;
    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx as u32 + 1;

        if in_block_string {
            if raw.contains("\"#") {
                in_block_string = false;
            }
            continue;
        }

        let trimmed = raw.trim_start();
        if !trimmed.starts_with("//") {
            if let Some(sym) = parse_decl(trimmed, "function", rel_path, line_no) {
                boundary.functions.push(sym);
            } else if let Some(sym) = parse_decl(trimmed, "class", rel_path, line_no) {
                boundary.classes.push(sym);
            }
        }

        // Enter a block string when it opens without closing on this line.
        if raw.matches("#\"").count() > raw.matches("\"#").count() {
            in_block_string = true;
        }
    }
}

/// Extract a `keyword Name` declaration from a trimmed line, returning the
/// symbol when the line begins with `keyword` followed by an identifier.
fn parse_decl(trimmed: &str, keyword: &str, rel_path: &str, line: u32) -> Option<BamlSymbol> {
    let rest = trimmed.strip_prefix(keyword)?;
    // Require whitespace after the keyword so `functional`/`classy` don't match.
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    // A valid identifier is non-empty and does not start with a digit.
    if name.is_empty() || name.chars().next().is_some_and(|c| c.is_numeric()) {
        return None;
    }
    let signature = trimmed
        .split('{')
        .next()
        .unwrap_or(trimmed)
        .trim_end()
        .to_string();
    Some(BamlSymbol {
        name,
        file_path: rel_path.to_string(),
        line,
        signature,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn parse(content: &str) -> BamlBoundary {
        let mut b = BamlBoundary::default();
        parse_source(content, "baml_src/test.baml", &mut b);
        b
    }

    #[test]
    fn test_parses_function_and_class() {
        let b = parse(
            r##"
class Resume {
  name string
  skills string[]
}

function ExtractResume(resume: string) -> Resume {
  client GPT4
  prompt #"Extract the resume"#
}
"##,
        );
        assert_eq!(b.functions.len(), 1);
        assert_eq!(b.functions[0].name, "ExtractResume");
        assert_eq!(b.functions[0].file_path, "baml_src/test.baml");
        assert_eq!(b.functions[0].line, 7);
        assert_eq!(b.classes.len(), 1);
        assert_eq!(b.classes[0].name, "Resume");
        assert_eq!(b.classes[0].line, 2);
    }

    #[test]
    fn test_ignores_keyword_in_multiline_prompt() {
        // A declaration-shaped line inside a block-string prompt must NOT match.
        let b = parse(
            r##"
function Real(x: string) -> string {
  client GPT4
  prompt #"
    function Fake(y: string) -> string {
    class NotAClass {
  "#
}
"##,
        );
        assert_eq!(b.functions.len(), 1, "only the real function should match");
        assert_eq!(b.functions[0].name, "Real");
        assert!(b.classes.is_empty(), "prompt-body class must be ignored");
    }

    #[test]
    fn test_ignores_comments_and_similar_words() {
        let b = parse("// function Commented() -> int {\nfunctional_helper thing\nclassifier x\n");
        assert!(b.functions.is_empty());
        assert!(b.classes.is_empty());
    }

    #[test]
    fn test_signature_strips_trailing_brace() {
        let b = parse("function Foo(a: int, b: string) -> Bar {\n");
        assert_eq!(
            b.functions[0].signature,
            "function Foo(a: int, b: string) -> Bar"
        );
    }

    #[test]
    fn test_scan_detects_surface_and_missing_client() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("baml_src")).unwrap();
        fs::write(
            root.join("baml_src/main.baml"),
            "function Classify(text: string) -> string {\n  client GPT4\n}\n",
        )
        .unwrap();

        let b = scan(root);
        assert!(b.baml_src_present);
        assert!(!b.client_generated, "no baml_client dir present");
        assert_eq!(b.functions.len(), 1);
        assert_eq!(b.functions[0].name, "Classify");
        assert_eq!(b.functions[0].file_path, "baml_src/main.baml");
    }

    #[test]
    fn test_scan_detects_generated_client() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("baml_src")).unwrap();
        fs::create_dir_all(root.join("baml_client")).unwrap();
        fs::write(
            root.join("baml_src/main.baml"),
            "function Classify(text: string) -> string {}\n",
        )
        .unwrap();
        fs::write(root.join("baml_client/__init__.py"), "b = object()\n").unwrap();

        let b = scan(root);
        assert!(b.baml_src_present);
        assert!(b.client_generated, "baml_client dir should be detected");
    }

    #[test]
    fn test_scan_empty_when_no_baml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.py"), "def f(): pass\n").unwrap();

        let b = scan(root);
        assert!(!b.baml_src_present);
        assert!(b.is_empty());
    }
}
