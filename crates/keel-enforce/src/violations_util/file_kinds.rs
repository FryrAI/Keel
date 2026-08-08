/// Check if a file is a benchmark file — harness-invoked, like tests, so
/// dead-code analysis doesn't apply.
pub fn is_bench_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with("benches/") || normalized.contains("/benches/")
}

/// Check if a file is a declaration/stub file: signature-only by definition,
/// so docstring and body-level checks don't apply (`.pyi` stubs, `.d.ts`
/// declarations).
pub fn is_stub_file(path: &str) -> bool {
    path.ends_with(".pyi")
        || path.ends_with(".d.ts")
        || path.ends_with(".d.mts")
        || path.ends_with(".d.cts")
}

/// True when `normalized` is a Cargo target ROOT under `dir/` (at the repo
/// root or under any crate): directly `dir/<name>.rs`, or the
/// `dir/<name>/main.rs` form of a multi-file target. Sibling files of a
/// multi-file target (`dir/<name>/util.rs`) are modules of that target — they
/// share its namespace and are NOT roots.
fn target_root_under(normalized: &str, dir: &str) -> bool {
    let prefix = format!("{dir}/");
    let rest = if let Some(r) = normalized.strip_prefix(&prefix) {
        r
    } else if let Some(pos) = normalized.find(&format!("/{prefix}")) {
        &normalized[pos + 1 + prefix.len()..]
    } else {
        return false;
    };
    match rest.split('/').collect::<Vec<_>>().as_slice() {
        [file] => file.ends_with(".rs"),
        [_, main] => *main == "main.rs",
        _ => false,
    }
}

/// True for a Cargo-recognized binary/build-script compilation root:
/// crate-root `build.rs`, `src/main.rs`, a `src/bin` target root, or an
/// `examples` target root. `.rs` only — the layout conventions are Cargo's,
/// and a `src/bin/cli.ts` is an ordinary module that can import its siblings.
fn is_cargo_binary_root(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    if !normalized.ends_with(".rs") {
        return false;
    }
    // `src/build.rs` is a plain module, not a build script: a crate-root
    // build.rs never sits under src/.
    let crate_root_build = normalized == "build.rs"
        || (normalized.ends_with("/build.rs")
            && !normalized.ends_with("/src/build.rs")
            && normalized != "src/build.rs");
    crate_root_build
        || normalized == "src/main.rs"
        || normalized.ends_with("/src/main.rs")
        || target_root_under(&normalized, "src/bin")
        || target_root_under(&normalized, "examples")
}

/// True when two files are compiled as SEPARATE Cargo units — a build script,
/// a `src/main.rs`, a `src/bin/*.rs`, an `examples/*.rs`.
///
/// Cargo builds each as its own independent crate, so a name defined in one is
/// invisible to the other and the two can never actually collide: there is no
/// ambiguity for a rename to resolve, and W002's "rename one" fix_hint would
/// be advice to make the code worse. Structural, not name-based — `main` is
/// merely the most common shared name between two binary targets, not the only
/// one, and a real copy-paste between them is still caught by W006's body
/// tiers, which do not care which crate a body lives in.
///
/// A pair involving an ordinary library file is NOT exempt: `src/lib.rs` and
/// everything below it compile into one unit, where a duplicate name is
/// exactly the ambiguity W002 exists to report.
pub fn distinct_compilation_units(a: &str, b: &str) -> bool {
    a != b && is_cargo_binary_root(a) && is_cargo_binary_root(b)
}

/// Check if a file path is a test file by language convention.
/// Patterns: *_test.go, test_*.py, *_test.py, conftest.py, *.test.ts,
/// *.spec.ts, *.test.js, *.spec.js, *_test.rs, *_tests.rs, *_tests_*.rs,
/// tests.rs
///
/// Now that `Definition::in_test_context` marks test symbols precisely per
/// grammar (Rust/Python/TS in the tree-sitter walk, Go in its Tier-2 pass),
/// this coarse filename check has narrowed to a fallback: it is the only signal
/// for a file whose `parse_file` returned no definitions, and a belt-and-braces
/// safety net for test-support code the per-definition AST rules do not
/// individually catch — pytest `@pytest.fixture` helpers with non-`test_`
/// names, Go/TS test utilities. It is also still the sole test signal for the
/// store-based cross-file filters, whose `GraphNode`s carry no context flags.
pub fn is_test_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");

    // Directory-based: files inside tests/, __tests__/, test/ directories
    // Check both mid-path ("/tests/") and top-level ("tests/") variants
    if normalized.contains("/tests/")
        || normalized.contains("/__tests__/")
        || normalized.contains("/test/")
        || normalized.starts_with("tests/")
        || normalized.starts_with("__tests__/")
        || normalized.starts_with("test/")
    {
        return true;
    }

    let basename = normalized.rsplit('/').next().unwrap_or(&normalized);

    // Go: *_test.go
    if basename.ends_with("_test.go") {
        return true;
    }
    // Python: test_*.py, *_test.py, or the pytest fixture-root conftest.py —
    // fixtures there are consumed via their parameter NAME by tests in other
    // files, so the graph shows zero callers even though pytest invokes them.
    if basename == "conftest.py"
        || (basename.ends_with(".py")
            && (basename.starts_with("test_") || basename.ends_with("_test.py")))
    {
        return true;
    }
    // TypeScript/JavaScript: *.test.ts, *.spec.ts, *.test.js, *.spec.js, *.test.tsx, *.spec.tsx
    if basename.contains(".test.") || basename.contains(".spec.") {
        return true;
    }
    // Rust: *_test.rs, *_tests.rs (plural — this repo's own `#[path =
    // "..._tests.rs"] mod tests;` convention), or exactly tests.rs.
    // Suffix match, not substring: "utils_tests.rs" matches, "contests.rs"
    // does not (no underscore before "tests.rs").
    // Also `*_tests_*.rs` — the `engine_tests_*.rs` split-file convention
    // (`engine_tests_e004_misc.rs`, `engine_tests_economy.rs`). `_tests_`
    // (underscore on both sides) stays precise: "contests_utils.rs" has
    // "ntests_", no leading underscore, so it does not match.
    if basename.ends_with("_test.rs")
        || basename.ends_with("_tests.rs")
        || basename == "tests.rs"
        || basename.contains("_tests_")
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cargo_binary_root_matches_recognized_roots() {
        for path in [
            "build.rs",
            "crates/x/build.rs",
            "src/main.rs",
            "crates/x/src/main.rs",
            "src/bin/foo.rs",
            "crates/x/src/bin/foo.rs",
            "examples/demo.rs",
            "crates/x/examples/demo.rs",
        ] {
            assert!(
                is_cargo_binary_root(path),
                "{path} should be a Cargo binary root"
            );
        }
    }

    #[test]
    fn test_is_cargo_binary_root_rejects_ordinary_source() {
        for path in [
            "src/lib.rs",
            "src/build_rs_helper.rs",
            "src/mainframe.rs",
            "crates/x/src/lib.rs",
        ] {
            assert!(
                !is_cargo_binary_root(path),
                "{path} must not match on substring"
            );
        }
    }

    #[test]
    fn test_distinct_compilation_units_needs_two_binary_roots() {
        // Two independent targets: invisible to each other, whatever the name.
        assert!(distinct_compilation_units("src/bin/a.rs", "src/bin/b.rs"));
        assert!(distinct_compilation_units("build.rs", "src/main.rs"));
        assert!(distinct_compilation_units(
            "examples/one.rs",
            "crates/x/src/bin/two.rs"
        ));
        // A library file on either side shares a namespace — not exempt.
        assert!(!distinct_compilation_units("src/bin/a.rs", "src/lib.rs"));
        assert!(!distinct_compilation_units("src/lib.rs", "src/util.rs"));
        // The same file is one unit, not two.
        assert!(!distinct_compilation_units("src/bin/a.rs", "src/bin/a.rs"));
    }

    #[test]
    fn test_cargo_binary_root_is_rust_only() {
        // The layout conventions are Cargo's; same-named dirs in other
        // languages hold ordinary modules that CAN import each other.
        for path in ["examples/a.ts", "src/bin/cli.ts", "examples/demo.py"] {
            assert!(!is_cargo_binary_root(path), "{path} is not a Cargo root");
        }
        assert!(!distinct_compilation_units(
            "examples/a.ts",
            "examples/b.ts"
        ));
    }

    #[test]
    fn test_cargo_binary_root_multi_file_targets_have_one_root() {
        // dir/<name>/main.rs is the root; its siblings are modules of it.
        assert!(is_cargo_binary_root("examples/multi/main.rs"));
        assert!(is_cargo_binary_root("src/bin/tool/main.rs"));
        for path in [
            "examples/multi/util.rs",
            "src/bin/tool/args.rs",
            "examples/multi/nested/main.rs",
        ] {
            assert!(!is_cargo_binary_root(path), "{path} is not a target root");
        }
        assert!(!distinct_compilation_units(
            "examples/multi/main.rs",
            "examples/multi/util.rs"
        ));
    }

    #[test]
    fn test_src_build_rs_is_a_module_not_a_build_script() {
        assert!(!is_cargo_binary_root("src/build.rs"));
        assert!(!is_cargo_binary_root("crates/x/src/build.rs"));
        assert!(!distinct_compilation_units("src/build.rs", "src/main.rs"));
    }

    #[test]
    fn test_is_stub_file() {
        assert!(is_stub_file("src/models.pyi"));
        assert!(is_stub_file("types/global.d.ts"));
        assert!(!is_stub_file("src/models.py"));
        assert!(!is_stub_file("src/app.ts"));
    }

    #[test]
    fn test_is_test_file() {
        // Go
        assert!(is_test_file("pkg/handler_test.go"));
        assert!(!is_test_file("pkg/handler.go"));

        // Python
        assert!(is_test_file("tests/test_handler.py"));
        assert!(is_test_file("src/handler_test.py"));
        assert!(!is_test_file("src/handler.py"));
        assert!(!is_test_file("src/testing_utils.py")); // not a test file

        // TypeScript/JavaScript
        assert!(is_test_file("src/handler.test.ts"));
        assert!(is_test_file("src/handler.spec.ts"));
        assert!(is_test_file("src/handler.test.js"));
        assert!(is_test_file("src/handler.spec.tsx"));
        assert!(!is_test_file("src/handler.ts"));

        // Rust
        assert!(is_test_file("src/handler_test.rs"));
        assert!(is_test_file("src/tests.rs"));
        assert!(!is_test_file("src/handler.rs"));

        // Rust: plural `*_tests.rs` — this repo's own
        // `#[path = "..._tests.rs"] mod tests;` convention.
        assert!(is_test_file("crates/keel-core/src/sqlite_tests.rs"));
        assert!(is_test_file("engine_tests.rs"));
        assert!(is_test_file("utils_tests.rs"));
        // Suffix match, not substring: no underscore before "tests.rs".
        assert!(!is_test_file("contests.rs"));
        assert!(!is_test_file("src/testing_utils.rs")); // not a test file

        // Rust: `*_tests_*.rs` split-file convention (engine_tests_*.rs) —
        // full paths and bare basenames.
        assert!(is_test_file(
            "crates/keel-enforce/src/engine_tests_e004_misc.rs"
        ));
        assert!(is_test_file("engine_tests_economy.rs"));
        assert!(is_test_file("engine_tests_batch_suppress.rs"));
        // `_tests_` needs an underscore on BOTH sides: "contests_utils.rs"
        // has "ntests_" (no leading underscore), so it does not match.
        assert!(!is_test_file("contests_utils.rs"));

        // Directory-based detection
        assert!(is_test_file("src/tests/helpers.py"));
        assert!(is_test_file("project/test/utils.go"));
        assert!(is_test_file("src/__tests__/handler.ts"));
        assert!(!is_test_file("src/contest/handler.py")); // "contest" != "test"

        // Top-level test directories (no parent path prefix)
        assert!(is_test_file("tests/helpers.py"));
        assert!(is_test_file("test/utils.go"));
        assert!(is_test_file("__tests__/handler.ts"));
    }

    #[test]
    fn test_is_test_file_conftest() {
        // pytest's fixture-root file: fixtures are consumed by parameter name
        // in other test files, so it must be recognized as a test file even
        // though its basename matches none of the test_*/{_test} patterns.
        assert!(is_test_file("conftest.py"));
        assert!(is_test_file("app/tests/conftest.py"));
        // Exact basename match only — a name that merely contains "conftest"
        // is ordinary production code.
        assert!(!is_test_file("myconftest.py"));
    }
}
