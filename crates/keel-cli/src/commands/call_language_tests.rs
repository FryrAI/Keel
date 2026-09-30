//! Admission probes for every ordinary rung and the boundary exemption.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use keel_parsers::resolver::{
    CallSite, Definition, Import, LanguageResolver, ParseResult, Reference, ReferenceKind,
    ResolvedEdge,
};

use super::*;
use crate::commands::call_resolve::resolve_call_reference;
use crate::commands::map_lang_resolve::ResolverSet;

#[derive(Default)]
struct Index {
    candidates: Vec<(String, u64)>,
    modules: HashMap<String, u64>,
    locals: HashMap<(String, String), u64>,
    packages: HashMap<String, HashMap<String, u64>>,
    boundary: HashMap<String, (u64, f64)>,
    associated: HashMap<u64, (String, u32)>,
}

impl CallIndex for Index {
    fn candidates(&self, name: &str) -> Cow<'_, [(String, u64)]> {
        if name == "run" {
            Cow::Borrowed(&self.candidates)
        } else {
            Cow::Borrowed(&[])
        }
    }
    fn associated_target(&self, id: u64) -> Option<(String, u32)> {
        self.associated.get(&id).cloned()
    }
    fn module_files(&self) -> &HashMap<String, u64> {
        &self.modules
    }
    fn name_to_id(&self) -> &HashMap<(String, String), u64> {
        &self.locals
    }
    fn package_index(&self) -> &HashMap<String, HashMap<String, u64>> {
        &self.packages
    }
    fn boundary_index(&self) -> &HashMap<String, (u64, f64)> {
        &self.boundary
    }
}

struct Resolver(&'static str);

impl LanguageResolver for Resolver {
    fn language(&self) -> &str {
        "python"
    }
    fn parse_file(&self, _: &Path, _: &str) -> ParseResult {
        panic!("not a parse probe")
    }
    fn resolve_definitions(&self, _: &Path) -> Vec<Definition> {
        vec![]
    }
    fn resolve_references(&self, _: &Path) -> Vec<Reference> {
        vec![]
    }
    fn resolve_call_edge(&self, _: &CallSite) -> Option<ResolvedEdge> {
        Some(ResolvedEdge {
            target_name: "run".into(),
            target_file: self.0.into(),
            confidence: 0.95,
            resolution_tier: "tier2".into(),
        })
    }
}

fn reference() -> Reference {
    Reference {
        name: "run".into(),
        file_path: "src/caller.py".into(),
        line: 7,
        kind: ReferenceKind::Call,
        resolved_to: None,
        call_arity: Some(1),
    }
}

fn import(source: &str) -> Import {
    Import {
        source: source.into(),
        imported_names: vec!["run".into()],
        file_path: "src/caller.py".into(),
        line: 1,
        is_relative: false,
    }
}

fn target(
    index: &Index,
    resolver: Option<&dyn LanguageResolver>,
    imports: &[Import],
) -> Option<u64> {
    let resolvers = ResolverSet {
        ts: None,
        py: resolver,
        go: None,
        rs: None,
    };
    let ctx = CallSiteCtx {
        resolvers: &resolvers,
        language: "python",
        file_path: "src/caller.py",
        abs_file: Path::new("/repo/src/caller.py"),
        imports,
        definitions: &[],
    };
    resolve_call_reference(index, &ctx, &reference()).map(|hit| hit.target_id)
}

#[test]
fn tier2_exact_and_unique_fallback_reject_foreign_candidates() {
    for path in ["foreign/run.rs", "foreign/run.go", "foreign/run.ts"] {
        let index = Index {
            candidates: vec![(path.into(), 1)],
            ..Index::default()
        };
        assert_eq!(target(&index, Some(&Resolver("unmatched")), &[]), None);
        // Use a real target path as well as the unique-name fallback.
        let resolvers = ResolverSet {
            ts: None,
            py: Some(&Resolver("foreign/run.rs")),
            go: None,
            rs: None,
        };
        let ctx = CallSiteCtx {
            resolvers: &resolvers,
            language: "python",
            file_path: "src/caller.py",
            abs_file: Path::new("/repo/src/caller.py"),
            imports: &[],
            definitions: &[],
        };
        assert!(resolve_call_reference(&index, &ctx, &reference()).is_none());
    }
}

#[test]
fn imports_same_directory_and_packages_reject_foreign_candidates() {
    let mut index = Index {
        candidates: vec![("src/run.rs".into(), 1)],
        ..Index::default()
    };
    assert_eq!(target(&index, None, &[import("src/run.rs")]), None);
    assert_eq!(target(&index, None, &[]), None);
    index.candidates[0].0 = "foreign/run.rs".into();
    index
        .packages
        .insert("pkg".into(), HashMap::from([("run".into(), 1)]));
    assert_eq!(target(&index, None, &[import("pkg")]), None);
    index.candidates[0].0 = "foreign/run.py".into();
    assert_eq!(target(&index, None, &[import("pkg")]), Some(1));
}

#[test]
fn rejected_foreign_pick_never_rebinds_to_later_rung_or_boundary() {
    let index = Index {
        candidates: vec![("foreign/run.rs".into(), 1), ("src/run.py".into(), 2)],
        boundary: HashMap::from([("run".into(), (3, 0.7))]),
        ..Index::default()
    };
    assert_eq!(target(&index, Some(&Resolver("foreign/run.rs")), &[]), None);
    // Without that earlier pick the ordinary same-directory base edge stays.
    assert_eq!(target(&index, None, &[]), Some(2));
}

#[test]
fn compatible_members_still_block_unique_fallback_and_base_ambiguity_stays() {
    for associated in [false, true] {
        let index = Index {
            candidates: vec![
                ("a/run.py".into(), 1),
                ("b/run.py".into(), 2),
                ("c/run.rs".into(), 3),
            ],
            associated: if associated {
                HashMap::from([(2, ("b/run.py".into(), 2))])
            } else {
                HashMap::new()
            },
            ..Index::default()
        };
        assert_eq!(target(&index, Some(&Resolver("unmatched")), &[]), None);
    }
    let index = Index {
        candidates: vec![("a/run.py".into(), 1), ("b/run.rs".into(), 2)],
        ..Index::default()
    };
    assert_eq!(target(&index, Some(&Resolver("unmatched")), &[]), None);
}

#[test]
fn boundary_rung_remains_cross_language() {
    let index = Index {
        boundary: HashMap::from([("run".into(), (3, 0.7))]),
        ..Index::default()
    };
    assert_eq!(target(&index, None, &[]), Some(3));
}

#[test]
fn canonical_script_languages_share_family_and_sql_is_unchanged() {
    let files = [
        "target.ts",
        "target.tsx",
        "target.js",
        "target.jsx",
        "target.mjs",
        "target.cjs",
        "target.svelte",
        "target.astro",
        "target.mts",
        "target.cts",
    ];
    for caller in files {
        let language = detect_language(Path::new(caller)).unwrap();
        for target in files {
            assert!(compatible(language, target), "{caller} -> {target}");
        }
        assert!(!compatible(language, "target.py"));
        assert!(!compatible(language, "target.rs"));
    }
    assert!(compatible("python", "target.pyi"));
    assert!(compatible("python", "target.sql"));
    assert!(compatible("sql", "target.rs"));
    assert!(compatible("sql", "target.unknown"));
    assert!(!compatible("python", "target.unknown"));
}

#[test]
fn foreign_rejection_blocks_tier3_admission() {
    use crate::commands::call_resolve::resolve_call_reference_with_rejection;
    let index = Index {
        candidates: vec![("src/run.rs".into(), 1)],
        ..Index::default()
    };
    let resolvers = ResolverSet {
        ts: None,
        py: None,
        go: None,
        rs: None,
    };
    let ctx = CallSiteCtx {
        resolvers: &resolvers,
        language: "python",
        file_path: "src/caller.py",
        abs_file: Path::new("/repo/src/caller.py"),
        imports: &[],
        definitions: &[],
    };
    let mut rejected = false;
    assert!(
        resolve_call_reference_with_rejection(&index, &ctx, &reference(), &mut rejected).is_none()
    );
    assert!(
        rejected,
        "Tier 3 must not replace a rejected earlier binding"
    );
}

#[test]
fn fresh_module_call_has_local_hash_before_any_graph_write() {
    use crate::commands::compile_sync::resolve_call_targets;
    use keel_core::sqlite::SqliteGraphStore;
    use keel_parsers::python::PyResolver;
    use keel_parsers::resolver::FileIndex;

    let py = PyResolver::new();
    let path = Path::new("/repo/tools/new.py");
    // A duplicate free definition discriminates map's last pick from the
    // first definition chosen by Python's resolver and compile's node writer.
    let content = "def run(a: str) -> str:\n    \"\"\"First.\"\"\"\n    return a\ndef run(a: str, b: str) -> str:\n    \"\"\"Last.\"\"\"\n    return a + b\nX = run(\"a\", \"b\")\n";
    let parsed = py.parse_file(path, content);
    let expected = parsed
        .definitions
        .iter()
        .rfind(|d| d.name == "run")
        .unwrap()
        .hash();
    let mut files = [FileIndex::from_parse("tools/new.py", content, parsed)];
    let store = SqliteGraphStore::in_memory().unwrap();
    let resolvers = ResolverSet {
        ts: None,
        py: Some(&py),
        go: None,
        rs: None,
    };
    resolve_call_targets(&store, Path::new("/repo"), &mut files, &resolvers);
    let call = files[0]
        .references
        .iter()
        .find(|r| r.name == "run" && r.line == 7)
        .unwrap();
    assert_eq!(call.resolved_to.as_deref(), Some(expected.as_str()));
}

#[test]
fn parsed_local_admits_function_body_calls_but_never_bare_members() {
    use keel_parsers::python::PyResolver;
    let py = PyResolver::new();
    for (content, expected) in [
        (
            "class Guard:\n    def run(self) -> None:\n        pass\nX = run()\n",
            false,
        ),
        (
            "def run() -> None:\n    pass\ndef main() -> None:\n    run()\n",
            true,
        ),
    ] {
        let parsed = py.parse_file(Path::new("/repo/src/caller.py"), content);
        let call = parsed.references.iter().find(|r| r.name == "run").unwrap();
        assert_eq!(
            module_local(call, "src/caller.py", &parsed.definitions).is_some(),
            expected
        );
    }
}

#[test]
fn selected_boundary_entry_keeps_ordinary_rung_confidence() {
    let mut index = Index {
        candidates: vec![("baml_src/main.baml".into(), 3)],
        boundary: HashMap::from([("run".into(), (3, 0.75))]),
        ..Index::default()
    };
    assert_eq!(target(&index, None, &[import("baml_client")]), Some(3));
    assert_eq!(
        target(&index, Some(&Resolver("baml_src/main.baml")), &[]),
        Some(3)
    );
    // A different boundary with the same name must not replace a rejected pick.
    index.boundary.insert("run".into(), (4, 0.75));
    assert_eq!(target(&index, None, &[import("baml_client")]), None);
}

#[test]
fn tier2_macro_and_alias_names_keep_compatible_resolved_target() {
    let index = Index {
        candidates: vec![("foreign/run.py".into(), 1)],
        ..Index::default()
    };
    let resolvers = ResolverSet {
        ts: None,
        py: Some(&Resolver("foreign/run.py")),
        go: None,
        rs: None,
    };
    let ctx = CallSiteCtx {
        resolvers: &resolvers,
        language: "python",
        file_path: "src/caller.py",
        abs_file: Path::new("/repo/src/caller.py"),
        imports: &[],
        definitions: &[],
    };
    for name in ["run!", "renamed"] {
        let mut call = reference();
        call.name = name.into();
        assert_eq!(
            resolve_call_reference(&index, &ctx, &call)
                .unwrap()
                .target_id,
            1
        );
    }
}
