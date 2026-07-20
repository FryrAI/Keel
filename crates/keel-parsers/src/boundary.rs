//! Generic boundary provider abstraction.
//!
//! A *boundary* is a surface keel has no tree-sitter grammar for but whose
//! declarations are still call targets — today BAML's `.baml` LLM functions,
//! potentially protobuf / GraphQL SDL / OpenAPI in future. Each provider scans
//! a repo for its boundary symbols and reports the confidence at which calls
//! resolving into that boundary should be recorded. The `keel map` pipeline
//! iterates a list of providers, materialises their symbols as boundary nodes,
//! and the shared resolution ladder matches unresolved calls against them.
//!
//! BAML is the single implementation ([`BamlProvider`]); it wraps the raw
//! [`crate::baml`] line scanner, which stays the internal primitive.

use std::path::Path;

use keel_core::types::NodeKind;

/// A declaration discovered at a language boundary — a call target keel has no
/// native grammar for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundarySymbol {
    /// Declared name (e.g. `ExtractResume`).
    pub name: String,
    /// Repo-relative path of the declaring file (forward slashes).
    pub file_path: String,
    /// 1-based line of the declaration.
    pub line: u32,
    /// Best-effort signature text (declaration line, minus any trailing `{`).
    pub signature: String,
    /// Whether the symbol is a callable ([`NodeKind::Function`]) or a type
    /// ([`NodeKind::Class`]). Only functions become call targets.
    pub kind: NodeKind,
}

/// A scanner for one kind of language boundary.
///
/// The `keel map` pipeline holds a list of these and, for each, materialises
/// the scanned symbols as boundary nodes and resolves unresolved calls into
/// them at [`BoundaryProvider::confidence`].
pub trait BoundaryProvider {
    /// Scan `root` for this boundary's declarations, returning every function
    /// and class symbol found (empty when the repo does not use this boundary).
    fn scan(&self, root: &Path) -> Vec<BoundarySymbol>;
    /// Confidence for a call edge resolved into this boundary. Deliberately
    /// warning-tier: boundary resolution is a cross-language name-match
    /// heuristic that exists to make the surface visible, never to hard-error.
    fn confidence(&self) -> f64;
}

/// The BAML boundary: `function` / `class` declarations in `baml_src/*.baml`.
pub struct BamlProvider;

impl BoundaryProvider for BamlProvider {
    /// Wrap [`crate::baml::scan`], converting its `function`/`class` findings
    /// into [`BoundarySymbol`]s. Also emits the "baml_src present but no
    /// generated client" hint here, so no BAML-specific logic remains in the
    /// map orchestration.
    fn scan(&self, root: &Path) -> Vec<BoundarySymbol> {
        let boundary = crate::baml::scan(root);
        if boundary.baml_src_present && !boundary.client_generated {
            eprintln!(
                "keel map: baml_src detected but no generated baml_client/baml_sdk found — run `baml generate` ({} BAML function(s) exposed as boundary stubs)",
                boundary.functions.len()
            );
        }
        let mut symbols = Vec::with_capacity(boundary.functions.len() + boundary.classes.len());
        for f in boundary.functions {
            symbols.push(BoundarySymbol {
                name: f.name,
                file_path: f.file_path,
                line: f.line,
                signature: f.signature,
                kind: NodeKind::Function,
            });
        }
        for c in boundary.classes {
            symbols.push(BoundarySymbol {
                name: c.name,
                file_path: c.file_path,
                line: c.line,
                signature: c.signature,
                kind: NodeKind::Class,
            });
        }
        symbols
    }

    fn confidence(&self) -> f64 {
        keel_core::confidence::BAML_BOUNDARY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_baml_provider_scan_maps_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("baml_src")).unwrap();
        fs::write(
            root.join("baml_src/main.baml"),
            "class Resume {\n  name string\n}\n\nfunction Classify(text: string) -> string {\n  client GPT4\n}\n",
        )
        .unwrap();

        let symbols = BamlProvider.scan(root);
        let func = symbols
            .iter()
            .find(|s| s.name == "Classify")
            .expect("Classify function symbol");
        assert_eq!(func.kind, NodeKind::Function);
        assert_eq!(func.file_path, "baml_src/main.baml");
        let class = symbols
            .iter()
            .find(|s| s.name == "Resume")
            .expect("Resume class symbol");
        assert_eq!(class.kind, NodeKind::Class);
    }

    #[test]
    fn test_baml_provider_confidence_is_boundary_constant() {
        assert_eq!(
            BamlProvider.confidence(),
            keel_core::confidence::BAML_BOUNDARY
        );
    }

    #[test]
    fn test_provider_is_object_safe() {
        // The map holds providers as `Box<dyn BoundaryProvider>` and iterates
        // them; prove that shape compiles and dispatches.
        let providers: Vec<Box<dyn BoundaryProvider>> = vec![Box::new(BamlProvider)];
        let dir = tempfile::tempdir().unwrap();
        for p in &providers {
            assert!(p.confidence() < 0.80);
            assert!(p.scan(dir.path()).is_empty());
        }
    }
}
