//! Regressions combining role-based ignore filtering with strict Git path decoding.

#[path = "common/mod.rs"]
mod common;

use common::{git, keel};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn compile_changed_non_ascii_paths_honor_nested_keelignore() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let clean = "def app(x: int) -> int:\n    \"\"\"Doc.\"\"\"\n    return x\n";
    write(root, "pkg/.keelignore", "thirdparty/\n");
    write(root, "pkg/thirdparty/größe.py", clean);
    write(root, "pkg/größe.py", clean);
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success());
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    assert!(keel(root, &["map"]).status.success());

    write(
        root,
        "pkg/thirdparty/größe.py",
        "def ignored(value):\n    return value\n",
    );
    for cwd in [root.to_path_buf(), root.join("pkg")] {
        let out = keel(&cwd, &["compile", "--changed", "--json"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
    }
    // The same non-ASCII filename outside the excluded subtree must be checked.
    write(
        root,
        "pkg/größe.py",
        "def fresh(value):\n    return value\n",
    );
    let out = keel(root, &["compile", "--changed", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("E002") && stdout.contains("pkg/größe.py"),
        "{stdout}"
    );
    assert!(!stdout.contains("thirdparty/"), "{stdout}");
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    fn fixture() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write(root, "src/base.rs", "/// Base.\npub fn base() {}\n");
        git(root, &["init", "-q"]);
        assert!(keel(root, &["init"]).status.success());
        let path = root.join(".keel/keel.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["homes"] = serde_json::json!([
            {"name":"sql","patterns":["CURRENT_DATE"],"home":"src/home/**","scope":"src/**"}
        ]);
        config["review"]["gate"] = serde_json::json!(["W011"]);
        fs::write(path, config.to_string()).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "base"]);
        dir
    }

    #[test]
    fn unreadable_rename_destination_cannot_cancel_new_homes_occurrences() {
        let dir = fixture();
        let root = dir.path();
        write(
            root,
            "src/old.rs",
            "pub fn helper() {\n    let _ = \"CURRENT_DATE\";\n}\n",
        );
        git(root, &["add", "src"]);
        git(root, &["commit", "-q", "-m", "homes base"]);
        fs::rename(
            root.join("src/old.rs"),
            root.join(OsStr::from_bytes(b"src/\xff.rs")),
        )
        .unwrap();
        write(
            root,
            "src/ok.rs",
            "pub fn fresh() {\n    let _ = \"CURRENT_DATE\";\n}\n",
        );
        git(root, &["add", "-A", "src"]);
        let out = keel(root, &["review", "--base", "HEAD", "--gate", "--json"]);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let review: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let hits = review["new_violations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["code"] == "W011")
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 1, "{review}");
        assert_eq!(hits[0]["file"], "src/ok.rs", "{review}");
        assert_eq!(hits[0]["line"], 2, "{review}");
        assert!(
            review["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "helper"
                    && c["kind"] == "removed"
                    && c["file"] == "src/old.rs"),
            "{review}"
        );
    }

    #[test]
    fn unreadable_head_paths_use_walker_verdict_for_warning_eligibility() {
        let dir = fixture();
        let root = dir.path();
        write(root, ".git/info/exclude", "src/excluded*.rs\n");
        // The hand model would keep this path, but the walker skips symlink leaves.
        std::os::unix::fs::symlink(
            "base.rs",
            root.join(OsStr::from_bytes(b"src/filtered\xff.rs")),
        )
        .unwrap();
        for path in [
            b"src/excluded\xff.rs".as_slice(),
            b"src/visible\xff.rs".as_slice(),
        ] {
            fs::write(root.join(OsStr::from_bytes(path)), "fn hidden() {}\n").unwrap();
        }
        git(root, &["add", "-f", "src"]);
        for args in [
            vec!["compile", "--changed", "--json"],
            vec!["review", "--base", "HEAD", "--json"],
        ] {
            let out = keel(root, &args);
            assert_eq!(out.status.code(), Some(0), "{out:?}");
            let stderr = String::from_utf8(out.stderr).unwrap();
            assert!(!stderr.contains("filtered"), "{stderr}");
            assert!(!stderr.contains("excluded"), "{stderr}");
            assert_eq!(
                stderr
                    .matches("not valid UTF-8: src/visible\\xff.rs")
                    .count(),
                1,
                "{stderr}"
            );
        }
    }

    #[test]
    fn unreadable_base_paths_use_hand_model_for_warning_eligibility() {
        let dir = fixture();
        let root = dir.path();
        let old = root.join(OsStr::from_bytes(b"src/filtered\xff.rs"));
        fs::write(&old, "/// Moved.\npub fn moved() {}\n").unwrap();
        git(root, &["add", "src"]);
        git(root, &["commit", "-q", "-m", "rename base"]);
        fs::rename(&old, root.join("src/new.rs")).unwrap();
        // What now occupies the old spelling must not decide a base-side verdict.
        fs::create_dir(old).unwrap();
        git(root, &["add", "-A", "src"]);
        let out = keel(root, &["review", "--base", "HEAD", "--json"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            stderr
                .matches("not valid UTF-8: src/filtered\\xff.rs")
                .count(),
            1,
            "{stderr}"
        );
        let review: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(
            review["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "moved" && c["kind"] == "added" && c["file"] == "src/new.rs"),
            "{review}"
        );
    }
}
