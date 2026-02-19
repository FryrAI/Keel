//! Load and index a SCIP protobuf file into queryable in-memory structures.
//!
//! Builds three look-up tables for fast resolution:
//! - `definitions` — (file, line, col) to `ScipDefinition`
//! - `symbol_to_defs` — symbol string to one or more definitions
//! - `file_occurrences` — file path to all occurrences in that file

use std::collections::HashMap;
use std::path::Path;

/// A definition extracted from the SCIP index.
#[derive(Debug, Clone)]
pub struct ScipDefinition {
    pub symbol: String,      // full SCIP symbol string
    pub file_path: String,   // repo-relative path
    pub line: u32,           // 0-based (SCIP convention)
    pub column: u32,         // 0-based
    pub name: String,        // simple name extracted from symbol
}

/// A single occurrence (reference or definition) within a file.
#[derive(Debug, Clone)]
pub struct ScipOccurrence {
    pub symbol: String,      // full SCIP symbol string
    pub line: u32,           // 0-based
    pub column: u32,         // 0-based
    pub is_definition: bool, // SymbolRole::Definition bit is set
}

/// In-memory index over a loaded SCIP protobuf file.
pub struct ScipIndex {
    pub definitions: HashMap<(String, u32, u32), ScipDefinition>, // (file, line, col)
    pub symbol_to_defs: HashMap<String, Vec<ScipDefinition>>,
    pub file_occurrences: HashMap<String, Vec<ScipOccurrence>>,
}

/// Bitmask value for the Definition role in the SCIP protobuf.
const SYMBOL_ROLE_DEFINITION: i32 = 1;

impl ScipIndex {
    /// Build an empty index (useful for testing).
    pub fn empty() -> Self {
        Self {
            definitions: HashMap::new(),
            symbol_to_defs: HashMap::new(),
            file_occurrences: HashMap::new(),
        }
    }

    /// Load a SCIP index from a `.scip` protobuf file on disk.
    ///
    /// Requires the `tier3` feature (pulls in the `scip` and `protobuf` crates).
    #[cfg(feature = "tier3")]
    pub fn load(path: &Path) -> Result<Self, String> {
        use protobuf::Message;
        use scip::types::Index;

        let bytes = std::fs::read(path)
            .map_err(|e| format!("failed to read SCIP index {:?}: {}", path, e))?;

        let index: Index = Index::parse_from_bytes(&bytes)
            .map_err(|e| format!("failed to deserialize SCIP index: {}", e))?;

        let mut me = Self::empty();

        for document in &index.documents {
            let file = &document.relative_path;
            for occurrence in &document.occurrences {
                // SCIP range: [line, start_col, end_col] (3) or
                //             [start_line, start_col, end_line, end_col] (4).
                let (line, col) = if occurrence.range.len() >= 2 {
                    (occurrence.range[0] as u32, occurrence.range[1] as u32)
                } else {
                    (0, 0)
                };

                let symbol_str = occurrence.symbol.clone();
                let is_def = (occurrence.symbol_roles & SYMBOL_ROLE_DEFINITION) != 0;

                me.file_occurrences
                    .entry(file.clone())
                    .or_default()
                    .push(ScipOccurrence {
                        symbol: symbol_str.clone(),
                        line,
                        column: col,
                        is_definition: is_def,
                    });

                if is_def {
                    let name = super::symbol::parse_symbol(&symbol_str)
                        .map(|s| super::symbol::symbol_name(&s))
                        .unwrap_or_default();

                    let def = ScipDefinition {
                        symbol: symbol_str.clone(),
                        file_path: file.clone(),
                        line,
                        column: col,
                        name,
                    };
                    me.definitions.insert((file.clone(), line, col), def.clone());
                    me.symbol_to_defs.entry(symbol_str).or_default().push(def);
                }
            }
        }

        Ok(me)
    }

    /// Resolve a reference at `(file, line)` whose callee matches `name`.
    ///
    /// 1. Find non-definition occurrences in `file` on `line`.
    /// 2. Pick the one whose symbol name matches `name`.
    /// 3. Return the first definition for that symbol.
    pub fn resolve_reference(&self, file: &str, line: u32, name: &str) -> Option<&ScipDefinition> {
        let occurrences = self.file_occurrences.get(file)?;

        let symbol = occurrences
            .iter()
            .filter(|occ| occ.line == line && !occ.is_definition)
            .find(|occ| {
                super::symbol::parse_symbol(&occ.symbol)
                    .map(|s| super::symbol::symbol_matches_name(&s, name))
                    .unwrap_or(false)
            })
            .map(|occ| &occ.symbol)?;

        self.symbol_to_defs.get(symbol)?.first()
    }

    /// Total number of definition entries.
    pub fn definition_count(&self) -> usize {
        self.definitions.len()
    }

    /// Number of files with recorded occurrences.
    pub fn file_count(&self) -> usize {
        self.file_occurrences.len()
    }
}

// Test helpers — construct index data without protobuf I/O.
#[cfg(test)]
impl ScipIndex {
    pub fn insert_definition(&mut self, def: ScipDefinition) {
        self.definitions
            .insert((def.file_path.clone(), def.line, def.column), def.clone());
        self.symbol_to_defs.entry(def.symbol.clone()).or_default().push(def);
    }

    pub fn insert_occurrence(&mut self, file: &str, occ: ScipOccurrence) {
        self.file_occurrences.entry(file.to_owned()).or_default().push(occ);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(symbol: &str, file: &str, line: u32, name: &str) -> ScipDefinition {
        ScipDefinition { symbol: symbol.into(), file_path: file.into(), line, column: 0, name: name.into() }
    }

    fn ref_occ(symbol: &str, line: u32) -> ScipOccurrence {
        ScipOccurrence { symbol: symbol.into(), line, column: 4, is_definition: false }
    }

    #[test]
    fn test_empty_index() {
        let idx = ScipIndex::empty();
        assert_eq!(idx.definition_count(), 0);
        assert_eq!(idx.file_count(), 0);
        assert!(idx.resolve_reference("src/a.ts", 10, "foo").is_none());
    }

    #[test]
    fn test_counts() {
        let mut idx = ScipIndex::empty();
        idx.insert_definition(def("sym#", "src/a.ts", 5, "myFunc"));
        idx.insert_definition(def("sym2#", "src/b.ts", 1, "otherFunc"));
        idx.insert_occurrence("src/a.ts", ref_occ("sym#", 10));
        assert_eq!(idx.definition_count(), 2);
        assert_eq!(idx.file_count(), 1);
    }

    #[test]
    fn test_resolve_reference() {
        let sym = "scip-typescript npm pkg 1.0.0 src/lib.ts/doWork#";
        let mut idx = ScipIndex::empty();
        idx.insert_definition(def(sym, "src/lib.ts", 20, "doWork"));
        idx.insert_occurrence("src/main.ts", ref_occ(sym, 5));

        // Hit: correct file, line, name.
        let result = idx.resolve_reference("src/main.ts", 5, "doWork").expect("should resolve");
        assert_eq!(result.name, "doWork");
        assert_eq!(result.file_path, "src/lib.ts");

        // Miss: wrong line, wrong name, unknown file.
        assert!(idx.resolve_reference("src/main.ts", 6, "doWork").is_none());
        assert!(idx.resolve_reference("src/main.ts", 5, "otherFunc").is_none());
        assert!(idx.resolve_reference("src/unknown.ts", 5, "doWork").is_none());
    }
}
