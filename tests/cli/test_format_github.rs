// Tests for `--format github` — the CI annotation protocol, emitted by the
// binary rather than by a post-processor in the composite action.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use crate::common::keel_bin;

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
    assert!(out.status.success(), "keel init failed");
    let out = Command::new(&keel)
        .arg("map")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "keel map failed");
    dir
}

#[test]
/// A violation renders as a GitHub workflow command with the documented shape.
fn compile_format_github_emits_annotations() {
    // Untyped, undocumented Python: E002 + E003.
    let dir = init_and_map(&[("src/app.py", "def handler(request):\n    return request\n")]);

    let out = Command::new(keel_bin())
        .args(["compile", "src/app.py", "--format", "github"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run keel compile");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let annotations: Vec<&str> = stdout.lines().filter(|l| l.starts_with("::")).collect();
    assert!(
        !annotations.is_empty(),
        "expected annotations, got: {stdout}"
    );
    for line in &annotations {
        assert!(
            line.starts_with("::error file=") || line.starts_with("::warning file="),
            "malformed annotation: {line}"
        );
        assert!(line.contains(",line="), "annotation needs a line: {line}");
        assert!(
            line.contains(",title=[") && line.contains("]::"),
            "annotation needs a [CODE] title: {line}"
        );
        assert!(
            line.contains("file=src/app.py,"),
            "annotation needs the file: {line}"
        );
    }
    // Every violation line is an annotation — no keel-format output leaks in.
    assert_eq!(
        annotations.len(),
        stdout.lines().filter(|l| !l.trim().is_empty()).count(),
        "github format must not mix in another formatter's output: {stdout}"
    );
}

#[test]
/// The clean-compile contract holds in every format: empty stdout, exit 0.
fn compile_format_github_is_silent_when_clean() {
    let dir = init_and_map(&[(
        "src/clean.ts",
        "/** Returns the input unchanged. */\nexport function clean(x: number): number { return x; }\n",
    )]);

    let out = Command::new(keel_bin())
        .args(["compile", "src/clean.ts", "--format", "github"])
        .current_dir(dir.path())
        .output()
        .expect("failed to run keel compile");

    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "");
}

#[test]
/// The composite action must never reach for a runtime keel does not ship.
///
/// keel's Article 1 and Principle 10 are both "single binary, zero runtime
/// dependencies"; the annotations it posts in CI must come from the binary.
fn the_composite_action_has_no_python_dependency() {
    let action = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".github/actions/keel/action.yml");
    let yaml = fs::read_to_string(&action).expect("the composite action must exist");
    assert!(
        !yaml.contains("python3") && !yaml.contains("python "),
        "action.yml must not shell out to python: {}",
        action.display()
    );
    assert!(
        yaml.contains("--format github"),
        "action.yml must use the native annotation format"
    );
}
