// Tests for `keel context` command — minimal structural context for safe editing

use std::fs;
use std::process::Command;
use std::time::Instant;

use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fallback = workspace.join("target/debug/keel");
    if fallback.exists() {
        return fallback;
    }
    let status = Command::new("cargo")
        .args(["build", "-p", "keel-cli"])
        .current_dir(&workspace)
        .status()
        .expect("Failed to build keel");
    assert!(status.success(), "Failed to build keel binary");
    fallback
}

fn init_and_map(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    let keel = keel_bin();
    let out = Command::new(&keel)
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// Cross-file test fixture: auth.ts calls crypto.ts, handlers.ts calls auth.ts.
fn cross_file_fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/crypto.ts",
            concat!(
                "export function decode_jwt(token: string): any {\n",
                "  return JSON.parse(atob(token.split('.')[1]));\n",
                "}\n",
            ),
        ),
        (
            "src/auth.ts",
            concat!(
                "import { decode_jwt } from './crypto';\n",
                "export function validate_token(token: string): boolean {\n",
                "  const payload = decode_jwt(token);\n",
                "  return payload.exp > Date.now() / 1000;\n",
                "}\n",
            ),
        ),
        (
            "src/handlers.ts",
            concat!(
                "import { validate_token } from './auth';\n",
                "export function handle_request(token: string): number {\n",
                "  return validate_token(token) ? 200 : 401;\n",
                "}\n",
            ),
        ),
    ]
}

#[test]
fn test_context_single_file() {
    let dir = init_and_map(&[(
        "src/utils.ts",
        concat!(
            "export function add(a: number, b: number): number { return a + b; }\n",
            "export function sub(a: number, b: number): number { return a - b; }\n",
        ),
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/utils.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");

    assert_eq!(
        output.status.code(),
        Some(0),
        "context should exit 0\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CONTEXT src/utils.ts"));
    assert!(stdout.contains("add"));
    assert!(stdout.contains("sub"));
}

#[test]
fn test_context_json_output() {
    let dir = init_and_map(&[(
        "src/utils.ts",
        "export function greet(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/utils.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context --json");

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("context --json should produce valid JSON");

    assert_eq!(json["command"], "context");
    assert_eq!(json["file"], "src/utils.ts");
    assert!(json["version"].is_string());

    let symbols = json["symbols"].as_array().expect("symbols should be array");
    assert!(!symbols.is_empty(), "should have at least one symbol");

    let first = &symbols[0];
    assert_eq!(first["name"], "greet");
    assert!(first["hash"].is_string());
    assert!(first["callers"].is_array());
    assert!(first["callees"].is_array());
}

#[test]
fn test_context_cross_file_callers() {
    let dir = init_and_map(&cross_file_fixture());
    let keel = keel_bin();

    // auth.ts should show callers from handlers.ts
    let output = Command::new(&keel)
        .args(["context", "src/auth.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let symbols = json["symbols"].as_array().unwrap();
    // validate_token should have external callers
    let validate = symbols
        .iter()
        .find(|s| s["name"] == "validate_token")
        .expect("should find validate_token");

    let callers = validate["callers"].as_array().unwrap();
    assert!(
        !callers.is_empty(),
        "validate_token should have external callers from handlers.ts"
    );
    // At least one caller should be from handlers.ts
    assert!(
        callers
            .iter()
            .any(|c| c["file"].as_str().unwrap().contains("handlers")),
        "should have a caller from handlers.ts: {:?}",
        callers
    );
}

#[test]
fn test_context_excludes_internal_edges() {
    let dir = init_and_map(&[(
        "src/internal.ts",
        concat!(
            "function helper(): void {}\n",
            "export function main_fn(): void { helper(); }\n",
        ),
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/internal.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let symbols = json["symbols"].as_array().unwrap();
    // All callers/callees should be empty since there are no cross-file edges
    for sym in symbols {
        let callers = sym["callers"].as_array().unwrap();
        let callees = sym["callees"].as_array().unwrap();
        // Internal edges should be filtered out
        for c in callers {
            assert_ne!(
                c["file"].as_str().unwrap(),
                "src/internal.ts",
                "should not include same-file callers"
            );
        }
        for c in callees {
            assert_ne!(
                c["file"].as_str().unwrap(),
                "src/internal.ts",
                "should not include same-file callees"
            );
        }
    }
}

#[test]
fn test_context_excludes_module_nodes() {
    let dir = init_and_map(&[(
        "src/utils.ts",
        "export function add(a: number, b: number): number { return a + b; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/utils.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let symbols = json["symbols"].as_array().unwrap();
    for sym in symbols {
        assert_ne!(
            sym["kind"].as_str().unwrap(),
            "module",
            "context should exclude module nodes"
        );
    }
}

#[test]
fn test_context_unknown_file_exits_2() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/nonexistent.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");

    assert_eq!(
        output.status.code(),
        Some(2),
        "context for unknown file should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no data") || stderr.contains("map"),
        "should hint about missing data: {stderr}"
    );
}

#[test]
fn test_context_not_initialized_exits_2() {
    let dir = TempDir::new().unwrap();
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");

    assert_eq!(
        output.status.code(),
        Some(2),
        "context without init should exit 2"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not initialized") || stderr.contains("init"),
        "should mention initialization: {stderr}"
    );
}

#[test]
fn test_context_verbose_shows_summary() {
    let dir = init_and_map(&[(
        "src/mod.ts",
        "export function a(): void {}\nexport function b(): void {}\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/mod.ts", "--verbose"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context --verbose");

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("symbols") && stderr.contains("ext callers"),
        "verbose should show summary counts on stderr: {stderr}"
    );
}

#[test]
fn test_context_llm_shows_signatures() {
    let dir = init_and_map(&[(
        "src/typed.ts",
        "export function compute(x: number, y: number): number { return x + y; }\n",
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/typed.ts", "--llm"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context --llm");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("SIG:"),
        "LLM output should include SIG: lines: {stdout}"
    );
}

#[test]
fn test_context_performance() {
    let dir = init_and_map(&[(
        "src/index.ts",
        "export function hello(name: string): string { return name; }\n",
    )]);
    let keel = keel_bin();

    let start = Instant::now();
    let _ = Command::new(&keel)
        .args(["context", "src/index.ts"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 5000,
        "context took {:?} — should be fast",
        elapsed
    );
}

#[test]
fn test_context_json_symbol_fields() {
    let dir = init_and_map(&[(
        "src/service.ts",
        concat!(
            "export class UserService {\n",
            "  getUser(id: string): string { return id; }\n",
            "}\n",
            "export function standalone(x: number): number { return x; }\n",
        ),
    )]);
    let keel = keel_bin();

    let output = Command::new(&keel)
        .args(["context", "src/service.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run keel context --json");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let symbols = json["symbols"].as_array().unwrap();
    assert!(
        !symbols.is_empty(),
        "should have symbols for class + function"
    );

    // Every symbol must have required fields
    for sym in symbols {
        assert!(sym["name"].is_string(), "name should be string");
        assert!(sym["hash"].is_string(), "hash should be string");
        assert!(sym["kind"].is_string(), "kind should be string");
        assert!(sym["line_start"].is_number(), "line_start should be number");
        assert!(sym["line_end"].is_number(), "line_end should be number");
        assert!(sym["is_public"].is_boolean(), "is_public should be bool");
        assert!(sym["callers"].is_array(), "callers should be array");
        assert!(sym["callees"].is_array(), "callees should be array");
    }
}
