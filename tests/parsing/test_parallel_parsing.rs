// Tests for parallel file parsing (Spec 001 - Tree-sitter Foundation)
//
// Verifies that resolvers are Send+Sync and produce correct results when
// shared across threads.

use std::path::Path;
use std::sync::Arc;
use std::thread;

use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::typescript::TsResolver;

/// Generate N distinct TypeScript source strings, each with a unique function.
fn generate_ts_sources(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            let filename = format!("file_{i}.ts");
            let source = format!(
                "function func_{i}(x: number): number {{ return x + {i}; }}\n",
            );
            (filename, source)
        })
        .collect()
}

#[test]
/// Parsing N files sequentially then in parallel should yield the same
/// definition counts.
fn test_parallel_correctness() {
    let sources = generate_ts_sources(10);

    // Sequential parse
    let seq_resolver = TsResolver::new();
    let mut seq_counts: Vec<usize> = Vec::new();
    for (filename, source) in &sources {
        let result = seq_resolver.parse_file(Path::new(filename), source);
        seq_counts.push(result.definitions.len());
    }

    // Parallel parse using Arc<TsResolver> across threads
    let par_resolver = Arc::new(TsResolver::new());
    let handles: Vec<_> = sources
        .iter()
        .map(|(filename, source)| {
            let resolver = Arc::clone(&par_resolver);
            let filename = filename.clone();
            let source = source.clone();
            thread::spawn(move || {
                let result = resolver.parse_file(Path::new(&filename), &source);
                result.definitions.len()
            })
        })
        .collect();

    let par_counts: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Both should produce same definition counts (2 per file: 1 module + 1 function)
    assert_eq!(seq_counts.len(), par_counts.len());
    for count in &seq_counts {
        assert_eq!(*count, 2, "each file should have 2 definitions (module + function)");
    }
    for count in &par_counts {
        assert_eq!(*count, 2, "each file should have 2 definitions (module + function)");
    }
}

#[test]
/// Parallel parsing should not be dramatically slower than sequential.
/// Uses a small corpus (50 files) that runs quickly even in debug builds.
fn test_parallel_speedup() {
    use std::time::Instant;

    let sources = generate_ts_sources(50);
    let resolver = Arc::new(TsResolver::new());

    // Sequential
    let start = Instant::now();
    for (filename, source) in &sources {
        resolver.parse_file(Path::new(filename), source);
    }
    let seq_time = start.elapsed();

    // Parallel
    let par_resolver = Arc::new(TsResolver::new());
    let start = Instant::now();
    let handles: Vec<_> = sources
        .iter()
        .map(|(filename, source)| {
            let r = Arc::clone(&par_resolver);
            let f = filename.clone();
            let s = source.clone();
            thread::spawn(move || r.parse_file(Path::new(&f), &s))
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let par_time = start.elapsed();

    // Parallel should not be dramatically slower (allow some overhead for small corpus)
    // On multi-core machines, parallel should be similar or faster
    assert!(
        par_time.as_millis() < seq_time.as_millis() * 5 + 100,
        "Parallel parsing should not be 5x slower than sequential: seq={}ms par={}ms",
        seq_time.as_millis(),
        par_time.as_millis()
    );
}

#[test]
/// Each language resolver should produce results for its respective language.
fn test_parallel_mixed_languages() {
    let ts_resolver = Arc::new(TsResolver::new());
    let py_resolver = Arc::new(PyResolver::new());
    let go_resolver = Arc::new(GoResolver::new());
    let rs_resolver = Arc::new(RustLangResolver::new());

    let ts = {
        let r = Arc::clone(&ts_resolver);
        thread::spawn(move || {
            r.parse_file(
                Path::new("app.ts"),
                "function hello(name: string): string { return name; }",
            )
        })
    };
    let py = {
        let r = Arc::clone(&py_resolver);
        thread::spawn(move || {
            r.parse_file(
                Path::new("app.py"),
                "def hello(name: str) -> str:\n    return name\n",
            )
        })
    };
    let go = {
        let r = Arc::clone(&go_resolver);
        thread::spawn(move || {
            r.parse_file(
                Path::new("app.go"),
                "package main\n\nfunc Hello(name string) string {\n\treturn name\n}\n",
            )
        })
    };
    let rs = {
        let r = Arc::clone(&rs_resolver);
        thread::spawn(move || {
            r.parse_file(
                Path::new("app.rs"),
                "pub fn hello(name: &str) -> String { name.to_string() }\n",
            )
        })
    };

    let ts_result = ts.join().unwrap();
    let py_result = py.join().unwrap();
    let go_result = go.join().unwrap();
    let rs_result = rs.join().unwrap();

    assert!(
        !ts_result.definitions.is_empty(),
        "TypeScript resolver should produce definitions"
    );
    assert!(
        !py_result.definitions.is_empty(),
        "Python resolver should produce definitions"
    );
    assert!(
        !go_result.definitions.is_empty(),
        "Go resolver should produce definitions"
    );
    assert!(
        !rs_result.definitions.is_empty(),
        "Rust resolver should produce definitions"
    );
}

#[test]
/// Parsing multiple files in parallel should not produce duplicate definition
/// names within the same file.
fn test_parallel_no_duplicate_nodes() {
    let sources = generate_ts_sources(10);
    let resolver = Arc::new(TsResolver::new());

    let handles: Vec<_> = sources
        .iter()
        .map(|(filename, source)| {
            let r = Arc::clone(&resolver);
            let filename = filename.clone();
            let source = source.clone();
            thread::spawn(move || {
                let result = r.parse_file(Path::new(&filename), &source);
                let names: Vec<String> =
                    result.definitions.iter().map(|d| d.name.clone()).collect();
                (filename, names)
            })
        })
        .collect();

    for handle in handles {
        let (filename, names) = handle.join().unwrap();
        // Within a single file, no duplicate definition names
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            names.len(),
            sorted.len(),
            "Duplicate definitions found in {filename}"
        );
    }
}

#[test]
/// A parse error in one file should not affect parsing of other files.
fn test_parallel_error_isolation() {
    let resolver = Arc::new(TsResolver::new());

    let valid_source = "function valid(x: number): number { return x; }";
    let invalid_source = "function broken(x { {{{{ return; }";

    let valid_handle = {
        let r = Arc::clone(&resolver);
        thread::spawn(move || r.parse_file(Path::new("valid.ts"), valid_source))
    };
    let invalid_handle = {
        let r = Arc::clone(&resolver);
        thread::spawn(move || r.parse_file(Path::new("invalid.ts"), invalid_source))
    };

    let valid_result = valid_handle.join().unwrap();
    let _invalid_result = invalid_handle.join().unwrap();

    // Valid file should still produce definitions regardless of the invalid
    // file being parsed concurrently.
    assert!(
        !valid_result.definitions.is_empty(),
        "Valid file should still produce definitions even when invalid file is parsed concurrently"
    );
    assert!(
        valid_result.definitions.iter().any(|d| d.name == "valid"),
        "valid file should contain definition for 'valid'"
    );
}

#[test]
/// Parallel parsing of 50 files should complete within a reasonable time.
/// Uses a small corpus that runs quickly even in debug builds.
fn test_parallel_performance_target() {
    use std::time::Instant;

    let sources = generate_ts_sources(50);
    let resolver = Arc::new(TsResolver::new());

    let start = Instant::now();
    let handles: Vec<_> = sources
        .iter()
        .map(|(filename, source)| {
            let r = Arc::clone(&resolver);
            let f = filename.clone();
            let s = source.clone();
            thread::spawn(move || {
                let result = r.parse_file(Path::new(&f), &s);
                assert!(
                    !result.definitions.is_empty(),
                    "Each file should produce definitions"
                );
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed();

    // 50 files should complete well under 10 seconds even in debug builds
    assert!(
        elapsed.as_secs() < 10,
        "Parallel parsing of 50 files took {:.1}s (target: <10s)",
        elapsed.as_secs_f64()
    );
}
