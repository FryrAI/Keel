//! SCIP Tier 3 provider.
//!
//! Loads a pre-built `.scip` protobuf index produced by a language indexer
//! (e.g. `scip-typescript`, `scip-python`, `rust-analyzer --scip`) and
//! resolves call sites with confidence 0.95.
//!
//! SCIP indexes are static snapshots; `invalidate_file` is a deliberate no-op.
//! Re-index with upstream tooling and construct a new `ScipProvider` to pick
//! up changes.

pub mod reader;
pub mod symbol;

use std::path::{Path, PathBuf};

use super::provider::{Tier3Provider, Tier3Result};
use crate::resolver::CallSite;

/// Tier 3 provider backed by a pre-built SCIP protobuf index.
pub struct ScipProvider {
    language: String,
    index_path: PathBuf,
    index: Option<reader::ScipIndex>,
}

impl ScipProvider {
    /// Create a new provider for `language`, loading the index at `index_path`.
    ///
    /// If the index cannot be loaded a warning is emitted and `is_available()`
    /// returns `false`.
    pub fn new(language: &str, index_path: PathBuf) -> Self {
        let index = Self::try_load_index(&index_path);
        Self {
            language: language.to_owned(),
            index_path,
            index,
        }
    }

    fn try_load_index(path: &Path) -> Option<reader::ScipIndex> {
        #[cfg(feature = "tier3")]
        {
            match reader::ScipIndex::load(path) {
                Ok(idx) => {
                    eprintln!(
                        "[keel] SCIP index loaded: {} defs, {} files",
                        idx.definition_count(),
                        idx.file_count(),
                    );
                    Some(idx)
                }
                Err(e) => {
                    eprintln!("[keel] WARN: could not load SCIP index {:?}: {}", path, e);
                    None
                }
            }
        }
        #[cfg(not(feature = "tier3"))]
        {
            let _ = path;
            None
        }
    }

    /// Returns a reference to the loaded SCIP index, if available.
    pub fn index(&self) -> Option<&reader::ScipIndex> {
        self.index.as_ref()
    }

    /// Returns the filesystem path to the SCIP index file.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }
}

impl Tier3Provider for ScipProvider {
    fn language(&self) -> &str {
        &self.language
    }

    fn is_available(&self) -> bool {
        self.index.is_some()
    }

    fn resolve(&self, call_site: &CallSite) -> Tier3Result {
        let index = match &self.index {
            Some(idx) => idx,
            None => return Tier3Result::Unavailable,
        };
        // CallSite lines are 1-based; SCIP ranges are 0-based.
        match index.resolve_reference(
            &call_site.file_path,
            call_site.line.saturating_sub(1),
            &call_site.callee_name,
        ) {
            Some(def) => Tier3Result::Resolved {
                target_file: def.file_path.clone(),
                target_name: def.name.clone(),
                confidence: 0.95,
                provider: "scip".to_owned(),
            },
            None => Tier3Result::Unresolved,
        }
    }

    /// SCIP indexes are static snapshots — no-op.
    fn invalidate_file(&self, _file_path: &Path) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader::{ScipDefinition, ScipIndex, ScipOccurrence};

    fn provider_with_index(language: &str, index: ScipIndex) -> ScipProvider {
        ScipProvider {
            language: language.to_owned(),
            index_path: PathBuf::from("/dev/null"),
            index: Some(index),
        }
    }

    fn no_index_provider() -> ScipProvider {
        ScipProvider {
            language: "typescript".into(),
            index_path: PathBuf::from("/none"),
            index: None,
        }
    }

    fn cs(file: &str, line: u32, name: &str) -> CallSite {
        CallSite {
            file_path: file.into(),
            line,
            callee_name: name.into(),
            receiver: None,
        }
    }

    #[test]
    fn test_no_index_is_unavailable() {
        let p = no_index_provider();
        assert!(!p.is_available());
        assert!(matches!(
            p.resolve(&cs("src/main.ts", 10, "foo")),
            Tier3Result::Unavailable
        ));
    }

    #[test]
    fn test_with_index_is_available() {
        assert!(provider_with_index("typescript", ScipIndex::empty()).is_available());
    }

    #[test]
    fn test_language_accessor() {
        assert_eq!(
            provider_with_index("python", ScipIndex::empty()).language(),
            "python"
        );
    }

    #[test]
    fn test_resolve_hit() {
        let sym = "scip-typescript npm pkg 1.0.0 src/lib.ts/processData#";
        let mut index = ScipIndex::empty();
        index.insert_definition(ScipDefinition {
            symbol: sym.into(),
            file_path: "src/lib.ts".into(),
            line: 14,
            column: 0,
            name: "processData".into(),
        });
        index.insert_occurrence(
            "src/main.ts",
            ScipOccurrence {
                symbol: sym.into(),
                line: 9,
                column: 4,
                is_definition: false,
            },
        );

        let p = provider_with_index("typescript", index);
        // CallSite line 10 (1-based) -> SCIP line 9 (0-based).
        match p.resolve(&cs("src/main.ts", 10, "processData")) {
            Tier3Result::Resolved {
                target_file,
                target_name,
                confidence,
                provider,
            } => {
                assert_eq!(target_file, "src/lib.ts");
                assert_eq!(target_name, "processData");
                assert!((confidence - 0.95).abs() < f64::EPSILON);
                assert_eq!(provider, "scip");
            }
            other => panic!("expected Resolved, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_miss_returns_unresolved() {
        let p = provider_with_index("typescript", ScipIndex::empty());
        assert!(matches!(
            p.resolve(&cs("src/main.ts", 10, "unknownFunc")),
            Tier3Result::Unresolved
        ));
    }

    #[test]
    fn test_invalidate_file_is_noop() {
        let p = provider_with_index("python", ScipIndex::empty());
        p.invalidate_file(Path::new("src/app.py"));
        assert!(p.is_available());
    }

    #[test]
    fn test_resolve_batch_delegates() {
        let sym = "scip-python python pkg 3.10 src/utils.py/helper#";
        let mut index = ScipIndex::empty();
        index.insert_definition(ScipDefinition {
            symbol: sym.into(),
            file_path: "src/utils.py".into(),
            line: 4,
            column: 0,
            name: "helper".into(),
        });
        index.insert_occurrence(
            "src/app.py",
            ScipOccurrence {
                symbol: sym.into(),
                line: 9,
                column: 0,
                is_definition: false,
            },
        );

        let p = provider_with_index("python", index);
        let results = p.resolve_batch(&[
            cs("src/app.py", 10, "helper"),
            cs("src/app.py", 99, "missing"),
        ]);
        assert_eq!(results.len(), 2);
        assert!(results[0].is_resolved());
        assert!(!results[1].is_resolved());
    }
}
