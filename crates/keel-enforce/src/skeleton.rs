//! `keel skeleton` (issue #21) — a compressed, signature-only view of a file.
//!
//! Reuses the existing single-file parse path (the same `LanguageResolver`
//! `compile` uses) to extract imports and function/class signatures, then
//! ranks them public-first for token-efficient output. Shared by the CLI
//! command and the `keel/skeleton` MCP tool so the two never diverge.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::{detect_language, is_typescript_family};
use keel_parsers::typescript::TsResolver;

use crate::types::{SkeletonResult, SkeletonSymbol};

/// Build a signature-only skeleton of `file_path` from its `content`.
///
/// `include_private` keeps non-public symbols; when it is false, only public
/// symbols are shown — unless *no* symbol is public (visibility unavailable for
/// the language, or the file has no public API), in which case all are shown so
/// the skeleton is never empty. `include_docs` populates docstrings.
///
/// Returns `None` when the file's language is not supported.
pub fn build_skeleton(
    root_dir: &Path,
    file_path: &Path,
    content: &str,
    include_private: bool,
    include_docs: bool,
) -> Option<SkeletonResult> {
    let lang = detect_language(file_path)?;

    // Construct resolvers lazily-ish: only the matched one does real work.
    let ts;
    let py;
    let go;
    let rs;
    let resolver: &dyn LanguageResolver = if is_typescript_family(lang) {
        ts = TsResolver::with_project_root(root_dir);
        &ts
    } else if lang == "python" {
        py = PyResolver::detect();
        &py
    } else if lang == "go" {
        go = GoResolver::new();
        &go
    } else if lang == "rust" {
        rs = RustLangResolver::new();
        &rs
    } else {
        return None;
    };

    let parsed = resolver.parse_file(file_path, content);

    // Imports: source specifiers in order, deduped.
    let mut imports: Vec<String> = Vec::new();
    for import in &parsed.imports {
        if !imports.contains(&import.source) {
            imports.push(import.source.clone());
        }
    }

    let mut symbols: Vec<SkeletonSymbol> = parsed
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .map(|d| SkeletonSymbol {
            kind: d.kind.to_string(),
            name: d.name.clone(),
            signature: d.signature.clone(),
            is_public: d.is_public,
            line: d.line_start,
            docstring: if include_docs {
                d.docstring.clone()
            } else {
                None
            },
        })
        .collect();

    // Visibility filter: public-only by default, but never return an empty set
    // just because a language reports nothing as public.
    if !include_private {
        let public: Vec<SkeletonSymbol> = symbols.iter().filter(|s| s.is_public).cloned().collect();
        if !public.is_empty() {
            symbols = public;
        }
    }

    // Rank public-first, then by source line.
    symbols.sort_by(|a, b| (!a.is_public).cmp(&!b.is_public).then(a.line.cmp(&b.line)));

    Some(SkeletonResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "skeleton".to_string(),
        file: file_path.to_string_lossy().to_string(),
        language: lang.to_string(),
        imports,
        symbols,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn skeleton_typescript_signatures_only_no_bodies() {
        let src = "import { z } from './z';\n\
             export function pub(a: number): string { const x = a + 1; return `${x}`; }\n\
             function priv_helper(): void { console.log('secret body'); }\n";
        let result = build_skeleton(&root(), Path::new("src/a.ts"), src, false, false).unwrap();
        assert_eq!(result.language, "typescript");
        assert_eq!(result.imports, vec!["./z".to_string()]);
        // public-only by default: pub is exported, priv_helper is not.
        assert!(result.symbols.iter().any(|s| s.name == "pub"));
        assert!(!result.symbols.iter().any(|s| s.name == "priv_helper"));
        // No body text leaks into the signature view.
        for s in &result.symbols {
            assert!(!s.signature.contains("return"));
            assert!(!s.signature.contains("console.log"));
        }
    }

    #[test]
    fn skeleton_private_flag_includes_private() {
        let src = "export function pub(a: number): string { return ''; }\n\
             function priv_helper(): void {}\n";
        let result = build_skeleton(&root(), Path::new("src/a.ts"), src, true, false).unwrap();
        assert!(result.symbols.iter().any(|s| s.name == "priv_helper"));
    }

    #[test]
    fn skeleton_docs_toggle() {
        let src =
            "def public_fn(a: int) -> str:\n    \"\"\"Does a thing.\"\"\"\n    return str(a)\n";
        let no_docs = build_skeleton(&root(), Path::new("m.py"), src, true, false).unwrap();
        let with_docs = build_skeleton(&root(), Path::new("m.py"), src, true, true).unwrap();
        let f_no = no_docs
            .symbols
            .iter()
            .find(|s| s.name == "public_fn")
            .unwrap();
        let f_yes = with_docs
            .symbols
            .iter()
            .find(|s| s.name == "public_fn")
            .unwrap();
        assert!(f_no.docstring.is_none());
        assert!(f_yes.docstring.is_some());
    }

    #[test]
    fn skeleton_unsupported_language_is_none() {
        assert!(build_skeleton(&root(), Path::new("a.txt"), "hello", false, false).is_none());
    }
}
