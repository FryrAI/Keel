//! Purpose: classify a path into the *kind* of file it is, so checks that only
//! make sense on hand-written source stop grading tests, generated clients and
//! schema files.
//!
//! Related: violations_util.rs (`is_test_file`), audit/structure.rs,
//! audit/discoverability.rs, keel-cli commands/compile.rs.
//!
//! The classification is derived from the canonical `detect_language` table in
//! `keel_parsers::treesitter` — this module deliberately does **not** restate
//! which extensions are source, it asks. The only extension list it owns is
//! [`UNTRACKED_SIGNIFICANT`], which by construction contains extensions
//! `detect_language` rejects, so the two tables cannot drift into disagreement.

use std::path::Path;

use keel_parsers::treesitter::detect_language;

use crate::violations_util::is_test_file;

/// Extensions keel has no grammar for but which still carry structural weight:
/// a `.sql` migration, a `.baml` LLM contract, a `.proto`/`.graphql` schema all
/// have call sites in tracked code. Editing one and getting exit 0 with empty
/// output is a false all-clear, so `keel compile` names them explicitly (see
/// [`untracked_language_notice`]).
///
/// Deliberately short: extensions that are *documentation* or *config*
/// (`.md`, `.json`, `.lock`, `.toml`, …) must never appear here, or the
/// post-edit hook gets noisier on exactly the edits that matter least.
pub const UNTRACKED_SIGNIFICANT: &[&str] = &["baml", "graphql", "proto", "sql"];

/// What kind of file a path is.
///
/// Only [`FileClass::Source`] is hand-written code in a language keel parses;
/// everything else is either machine-generated, harness-invoked, or a
/// declarative surface where "function too long" and "too much of this file is
/// public" are category errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    /// Hand-written code in a language keel parses.
    Source,
    /// A declarative interface surface keel has no grammar for (`.baml`,
    /// `.proto`, `.graphql`).
    Boundary,
    /// Machine-generated code (generated client directories).
    Generated,
    /// Data/DDL (`.sql`).
    Data,
    /// Test code, per `violations_util::is_test_file`.
    Test,
}

impl FileClass {
    /// Classify a repo-relative (or absolute) path.
    ///
    /// Order matters: a `.ts` file inside a generated client dir is
    /// `Generated`, not `Source`, and a `.baml` file under `tests/` is `Test`,
    /// not `Boundary` — in both cases the stricter exemption wins, which is the
    /// safe direction for a check whose false positives are the problem.
    pub fn classify(path: &str) -> FileClass {
        let normalized = path.replace('\\', "/");

        if normalized
            .split('/')
            .any(|c| keel_parsers::baml::CLIENT_DIRS.contains(&c))
        {
            return FileClass::Generated;
        }
        if is_test_file(&normalized) {
            return FileClass::Test;
        }
        if detect_language(Path::new(&normalized)).is_some() {
            return FileClass::Source;
        }
        match untracked_significant_ext(&normalized) {
            Some("sql") => FileClass::Data,
            Some(_) => FileClass::Boundary,
            // Anything else keel cannot parse (docs, config, assets). It never
            // reaches the audit — only parsed files have graph nodes — so the
            // permissive default costs nothing and keeps the exemptions narrow.
            None => FileClass::Source,
        }
    }

    /// Whether size- and naming-shaped audit checks (`function_size`,
    /// `god_file`, `public_ratio`, `cryptic_name`) apply to this file.
    ///
    /// False for everything but [`FileClass::Source`]: a 253-line integration
    /// test is not a maintainability defect, a generated client's shape is not
    /// the author's decision, and "percent public" is not a concept a `.baml`
    /// or `.sql` file has.
    pub fn grades_size_and_naming(&self) -> bool {
        matches!(self, FileClass::Source)
    }
}

/// The [`UNTRACKED_SIGNIFICANT`] extension of `path`, if it has one.
///
/// Returns `None` for anything `detect_language` recognizes, so a future
/// grammar for one of these extensions silently retires its notice instead of
/// double-reporting a file keel now actually checks.
pub fn untracked_significant_ext(path: &str) -> Option<&'static str> {
    let p = Path::new(path);
    if detect_language(p).is_some() {
        return None;
    }
    let ext = p.extension()?.to_str()?.to_ascii_lowercase();
    UNTRACKED_SIGNIFICANT.iter().copied().find(|e| *e == ext)
}

/// One stderr line naming the structurally significant extensions among
/// `paths` that keel did not check, or `None` when there are none.
///
/// `keel compile` exits 0 for a file it has no grammar for, and a hook that
/// reads exit 0 as verification then reports an unchecked `.sql` migration as
/// verified. The exit code stays 0 — nothing failed — but it is never again a
/// silent all-clear.
pub fn untracked_language_notice(paths: &[String]) -> Option<String> {
    let mut exts: Vec<&'static str> = Vec::new();
    for path in paths {
        if let Some(ext) = untracked_significant_ext(path) {
            if !exts.contains(&ext) {
                exts.push(ext);
            }
        }
    }
    if exts.is_empty() {
        return None;
    }
    exts.sort_unstable();
    let list = exts
        .iter()
        .map(|e| format!(".{}", e))
        .collect::<Vec<_>>()
        .join(", ");
    Some(if exts.len() == 1 {
        format!("keel: {} is not a tracked language — no checks ran", list)
    } else {
        format!("keel: {} are not tracked languages — no checks ran", list)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_files_are_source() {
        for path in [
            "src/main.rs",
            "src/app.ts",
            "src/app.tsx",
            "src/app.js",
            "src/App.svelte",
            "app/models.py",
            "cmd/server/main.go",
        ] {
            assert_eq!(
                FileClass::classify(path),
                FileClass::Source,
                "{path} should be Source"
            );
        }
    }

    #[test]
    fn test_files_are_test() {
        assert_eq!(
            FileClass::classify("tests/cli/test_audit.rs"),
            FileClass::Test
        );
        assert_eq!(FileClass::classify("pkg/handler_test.go"), FileClass::Test);
        assert_eq!(FileClass::classify("src/app.spec.ts"), FileClass::Test);
    }

    #[test]
    fn boundary_generated_and_data() {
        assert_eq!(
            FileClass::classify("baml_src/main.baml"),
            FileClass::Boundary
        );
        assert_eq!(FileClass::classify("api/schema.proto"), FileClass::Boundary);
        assert_eq!(
            FileClass::classify("api/schema.graphql"),
            FileClass::Boundary
        );
        assert_eq!(FileClass::classify("migrations/001.sql"), FileClass::Data);
        // Generated wins over the extension's own class.
        assert_eq!(
            FileClass::classify("baml_client/async_client.py"),
            FileClass::Generated
        );
        assert_eq!(
            FileClass::classify("crates/core/baml_sdk/mod.rs"),
            FileClass::Generated
        );
    }

    #[test]
    fn windows_separators_classify_the_same() {
        assert_eq!(
            FileClass::classify("baml_client\\async_client.py"),
            FileClass::Generated
        );
        assert_eq!(
            FileClass::classify("migrations\\001_init.sql"),
            FileClass::Data
        );
    }

    #[test]
    fn only_source_is_graded() {
        let graded = |p: &str| FileClass::classify(p).grades_size_and_naming();
        assert!(graded("src/main.rs"));
        assert!(!graded("tests/cli/test_audit.rs"));
        assert!(!graded("baml_src/main.baml"));
        assert!(!graded("baml_client/client.py"));
        assert!(!graded("migrations/001.sql"));
    }

    /// The anti-drift guarantee: the untracked table may never claim an
    /// extension the canonical `detect_language` table already owns.
    #[test]
    fn untracked_table_is_disjoint_from_detect_language() {
        for ext in UNTRACKED_SIGNIFICANT {
            let path = format!("sample.{}", ext);
            assert!(
                detect_language(Path::new(&path)).is_none(),
                ".{ext} is both a tracked language and an untracked-significant extension"
            );
            assert_eq!(untracked_significant_ext(&path), Some(*ext));
        }
    }

    #[test]
    fn notice_fires_only_for_significant_extensions() {
        let notice = untracked_language_notice(&["migrations/001_init.sql".into()]);
        assert_eq!(
            notice.as_deref(),
            Some("keel: .sql is not a tracked language — no checks ran")
        );

        // Docs, config and lockfiles must never produce a notice.
        assert_eq!(
            untracked_language_notice(&[
                "README.md".into(),
                "package.json".into(),
                "Cargo.lock".into(),
                "src/main.rs".into(),
            ]),
            None
        );
    }

    #[test]
    fn notice_is_one_line_for_several_extensions() {
        let notice = untracked_language_notice(&[
            "migrations/001.sql".into(),
            "migrations/002.sql".into(),
            "baml_src/main.baml".into(),
        ])
        .expect("expected a notice");
        assert_eq!(notice.lines().count(), 1);
        assert_eq!(
            notice,
            "keel: .baml, .sql are not tracked languages — no checks ran"
        );
    }
}
