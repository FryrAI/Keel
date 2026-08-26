//! The two-sided symbol diff: base blobs vs the working tree.
//!
//! Both sides go through the same Tier-1 resolvers (`crate::parse_util`), so a
//! reported difference is a real difference and not an artifact of one side
//! having had `ty` or oxc available and the other not.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::{Definition, FileIndex};

use crate::file_class::FileClass;
use crate::gitdiff::{self, ChangeStatus, ChangedPath};
use crate::parse_util::BlobParser;

use super::{ChangeKind, ContractChange, UnanalyzedFile};

/// Extensions that carry no structural weight in a review.
///
/// Prose and lockfiles would otherwise dominate the UNANALYZED section on every
/// PR, which is exactly the noise that makes a cover letter unread. Everything
/// else keel cannot parse — `.sql`, `.baml`, fixture JSON, CI YAML — is named.
const UNANALYZED_IGNORED_EXTS: &[&str] = &["md", "markdown", "txt", "lock"];

/// Everything one pass over the diff produces.
pub struct DiffScan {
    /// Per-symbol deltas, unranked and without caller data yet.
    pub changes: Vec<ContractChange>,
    /// Changed files keel has no grammar for.
    pub unanalyzed: Vec<UnanalyzedFile>,
    /// Every path in the diff, both sides — the set a caller must live outside
    /// of to count as "not updated by this PR".
    pub diff_files: HashSet<String>,
    /// How many paths keel parsed on at least one side.
    pub files_analyzed: usize,
    /// The parsed base side, kept so the baseline violation diff
    /// (`super::baseline`) does not re-parse every blob a second time.
    pub base_indices: Vec<FileIndex>,
    /// The parsed head side, same reason.
    pub head_indices: Vec<FileIndex>,
    /// Head path → base path for every renamed file. Without it a rename would
    /// make every finding in the moved file read as newly introduced.
    pub renames: BTreeMap<String, String>,
}

/// The facts about one definition that a review compares.
struct Facts {
    kind: NodeKind,
    signature: String,
    hash: String,
    body_hash: u64,
    docstring: String,
    is_public: bool,
}

impl Facts {
    fn from_definition(def: &Definition) -> Self {
        Facts {
            kind: def.kind.clone(),
            signature: def.signature.clone(),
            hash: def.hash(),
            body_hash: xxhash_rust::xxh64::xxh64(def.body_for_hash().as_bytes(), 0),
            docstring: def.docstring.clone().unwrap_or_default(),
            is_public: def.is_public,
        }
    }
}

/// Index a parsed file by symbol name, first definition winning.
///
/// Modules are skipped: a module node has no signature a caller depends on.
/// Overloads and same-named associated items collapse onto the first
/// occurrence, matching how `checkpoint::diff_changed_files` reads a file.
fn facts_by_name(index: &FileIndex) -> BTreeMap<String, Facts> {
    let mut out: BTreeMap<String, Facts> = BTreeMap::new();
    for def in &index.definitions {
        if def.kind == NodeKind::Module {
            continue;
        }
        out.entry(def.name.clone())
            .or_insert_with(|| Facts::from_definition(def));
    }
    out
}

/// Which unanalyzed class `path` belongs to, or `None` when keel parses it (or
/// when it is prose keel deliberately stays quiet about).
///
/// Delegates the classification to [`FileClass`] so the review never restates
/// which directories are generated or which paths are tests.
fn unanalyzed_class(path: &str) -> Option<&'static str> {
    if keel_parsers::treesitter::detect_language(Path::new(path)).is_some() {
        return None;
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    if UNANALYZED_IGNORED_EXTS.contains(&ext.as_str()) {
        return None;
    }
    Some(match FileClass::classify(path) {
        FileClass::Generated => "generated",
        FileClass::Test => "fixture",
        FileClass::Boundary => "boundary",
        FileClass::Data => "data",
        FileClass::Source => "unparsed",
    })
}

/// Read the head-side content of `path`, or `None` when it no longer exists.
fn head_content(dir: &Path, path: &str, status: &ChangeStatus) -> Option<String> {
    if *status == ChangeStatus::Deleted {
        return None;
    }
    std::fs::read_to_string(dir.join(path)).ok()
}

/// Classify one symbol present on both sides.
fn kind_for_pair(base: &Facts, head: &Facts, moved_from: Option<&str>) -> Option<ChangeKind> {
    if base.signature != head.signature {
        return Some(ChangeKind::SignatureChanged);
    }
    if base.body_hash != head.body_hash {
        return Some(ChangeKind::BodyOnly);
    }
    if base.docstring != head.docstring {
        return Some(ChangeKind::DocOnly);
    }
    moved_from.map(|from| ChangeKind::Moved {
        from: from.to_string(),
    })
}

/// Diff one file's two sides into per-symbol changes.
fn diff_one_file(
    path: &str,
    base: &BTreeMap<String, Facts>,
    head: &BTreeMap<String, Facts>,
    moved_from: Option<&str>,
) -> Vec<ContractChange> {
    let mut out = Vec::new();

    for (name, h) in head {
        let (kind, b) = match base.get(name) {
            None => (ChangeKind::Added, None),
            Some(b) => match kind_for_pair(b, h, moved_from) {
                Some(k) => (k, Some(b)),
                // Byte-identical symbol in a touched file: not news.
                None => continue,
            },
        };
        out.push(ContractChange {
            name: name.clone(),
            symbol_kind: h.kind.clone(),
            file: path.to_string(),
            kind,
            sig_base: b.map(|b| b.signature.clone()),
            sig_head: Some(h.signature.clone()),
            hash_base: b.map(|b| b.hash.clone()),
            hash_head: Some(h.hash.clone()),
            is_public: h.is_public,
            callers_outside_diff: Vec::new(),
            callers_outside_diff_count: 0,
        });
    }

    for (name, b) in base {
        if head.contains_key(name) {
            continue;
        }
        out.push(ContractChange {
            name: name.clone(),
            symbol_kind: b.kind.clone(),
            file: path.to_string(),
            kind: ChangeKind::Removed,
            sig_base: Some(b.signature.clone()),
            sig_head: None,
            hash_base: Some(b.hash.clone()),
            hash_head: None,
            is_public: b.is_public,
            callers_outside_diff: Vec::new(),
            callers_outside_diff_count: 0,
        });
    }

    out
}

/// Walk every changed path, parsing both sides and collecting the deltas.
pub fn scan_paths(dir: &Path, base_ref: &str, paths: &[ChangedPath]) -> DiffScan {
    let mut parser = BlobParser::new();
    let mut changes = Vec::new();
    let mut unanalyzed = Vec::new();
    let mut diff_files: HashSet<String> = HashSet::new();
    let mut files_analyzed = 0usize;
    let mut base_indices: Vec<FileIndex> = Vec::new();
    let mut head_indices: Vec<FileIndex> = Vec::new();
    let mut renames: BTreeMap<String, String> = BTreeMap::new();

    for changed in paths {
        diff_files.insert(changed.path.clone());
        if let Some(from) = changed.base_path() {
            diff_files.insert(from.to_string());
        }

        if let Some(class) = unanalyzed_class(&changed.path) {
            unanalyzed.push(UnanalyzedFile {
                path: changed.path.clone(),
                class: class.to_string(),
            });
            continue;
        }

        let base_index = changed
            .base_path()
            .and_then(|p| gitdiff::blob_at(dir, base_ref, p).map(|c| (p.to_string(), c)))
            .and_then(|(p, content)| parser.parse(&p, &content));
        let base_facts = base_index.as_ref().map(facts_by_name).unwrap_or_default();

        let head_index = head_content(dir, &changed.path, &changed.status)
            .and_then(|content| parser.parse(&changed.path, &content));
        let head_facts = head_index.as_ref().map(facts_by_name).unwrap_or_default();

        base_indices.extend(base_index);
        head_indices.extend(head_index);

        if base_facts.is_empty() && head_facts.is_empty() {
            continue;
        }
        files_analyzed += 1;

        let moved_from = match &changed.status {
            ChangeStatus::Renamed { from } => Some(from.as_str()),
            _ => None,
        };
        if let Some(from) = moved_from {
            renames.insert(changed.path.clone(), from.to_string());
        }
        changes.extend(diff_one_file(
            &changed.path,
            &base_facts,
            &head_facts,
            moved_from,
        ));
    }

    unanalyzed.sort_by(|a, b| a.path.cmp(&b.path));
    DiffScan {
        changes,
        unanalyzed,
        diff_files,
        files_analyzed,
        base_indices,
        head_indices,
        renames,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_and_lockfiles_are_not_unanalyzed_noise() {
        assert_eq!(unanalyzed_class("README.md"), None);
        assert_eq!(unanalyzed_class("Cargo.lock"), None);
        assert_eq!(unanalyzed_class("notes.txt"), None);
        // Parsed languages are never "unanalyzed".
        assert_eq!(unanalyzed_class("src/main.rs"), None);
        assert_eq!(unanalyzed_class("migrations/001.sql"), None);
        // No extension at all: not worth a line.
        assert_eq!(unanalyzed_class("LICENSE"), None);
    }

    #[test]
    fn structural_non_source_is_named() {
        assert_eq!(unanalyzed_class("baml_src/main.baml"), Some("boundary"));
        assert_eq!(
            unanalyzed_class("tests/fixtures/eval_01.json"),
            Some("fixture")
        );
        assert_eq!(
            unanalyzed_class(".github/workflows/ci.yml"),
            Some("unparsed")
        );
    }

    fn facts(sig: &str, body: u64, doc: &str) -> Facts {
        Facts {
            kind: NodeKind::Function,
            signature: sig.to_string(),
            hash: format!("{}-{}-{}", sig, body, doc),
            body_hash: body,
            docstring: doc.to_string(),
            is_public: true,
        }
    }

    #[test]
    fn pair_classification_is_most_consequential_first() {
        let a = facts("fn f(x: u8)", 1, "doc");
        assert_eq!(
            kind_for_pair(&a, &facts("fn f(x: u8, y: u8)", 9, "other"), None),
            Some(ChangeKind::SignatureChanged)
        );
        assert_eq!(
            kind_for_pair(&a, &facts("fn f(x: u8)", 2, "doc"), None),
            Some(ChangeKind::BodyOnly)
        );
        assert_eq!(
            kind_for_pair(&a, &facts("fn f(x: u8)", 1, "new doc"), None),
            Some(ChangeKind::DocOnly)
        );
        assert_eq!(
            kind_for_pair(&a, &facts("fn f(x: u8)", 1, "doc"), None),
            None
        );
        assert_eq!(
            kind_for_pair(&a, &facts("fn f(x: u8)", 1, "doc"), Some("old.rs")),
            Some(ChangeKind::Moved {
                from: "old.rs".into()
            })
        );
        // A rename that also changed the contract reports the contract.
        assert_eq!(
            kind_for_pair(&a, &facts("fn f()", 1, "doc"), Some("old.rs")),
            Some(ChangeKind::SignatureChanged)
        );
    }

    #[test]
    fn added_and_removed_symbols_are_both_reported() {
        let mut base = BTreeMap::new();
        base.insert("gone".to_string(), facts("fn gone()", 1, ""));
        let mut head = BTreeMap::new();
        head.insert("fresh".to_string(), facts("fn fresh()", 2, ""));

        let out = diff_one_file("src/lib.rs", &base, &head, None);
        assert_eq!(out.len(), 2);
        let fresh = out.iter().find(|c| c.name == "fresh").unwrap();
        assert_eq!(fresh.kind, ChangeKind::Added);
        assert!(fresh.sig_base.is_none());
        let gone = out.iter().find(|c| c.name == "gone").unwrap();
        assert_eq!(gone.kind, ChangeKind::Removed);
        assert!(gone.sig_head.is_none());
    }
}
