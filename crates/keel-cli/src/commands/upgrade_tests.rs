use super::*;

#[test]
fn platform_artifact_returns_ok() {
    // Should always succeed on a supported platform (Linux/macOS, x86_64/aarch64)
    let result = platform_artifact();
    assert!(result.is_ok(), "platform_artifact() failed: {:?}", result);
    let artifact = result.unwrap();
    assert!(artifact.starts_with("keel-"));
    // Must contain a platform and architecture component
    assert!(
        artifact.contains("linux") || artifact.contains("darwin"),
        "unexpected artifact: {artifact}"
    );
    assert!(
        artifact.contains("amd64") || artifact.contains("arm64"),
        "unexpected artifact: {artifact}"
    );
}

#[test]
fn artifact_name_per_target() {
    // Names must match the release workflow matrix in .github/workflows/release.yml.
    assert_eq!(
        artifact_name("linux", "x86_64").unwrap(),
        "keel-linux-amd64"
    );
    assert_eq!(
        artifact_name("linux", "aarch64").unwrap(),
        "keel-linux-arm64"
    );
    assert_eq!(
        artifact_name("macos", "x86_64").unwrap(),
        "keel-darwin-amd64"
    );
    assert_eq!(
        artifact_name("macos", "aarch64").unwrap(),
        "keel-darwin-arm64"
    );
    assert_eq!(
        artifact_name("windows", "x86_64").unwrap(),
        "keel-windows-amd64.exe"
    );

    // Unsupported OS / arch are rejected.
    assert!(artifact_name("freebsd", "x86_64").is_err());
    assert!(artifact_name("linux", "mips").is_err());
}

#[test]
fn checksum_download_failure_is_fatal_and_installs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let tmp_binary = dir.path().join("keel.tmp");
    let tmp_checksums = dir.path().join("keel.checksums");

    // Simulate an already-downloaded binary awaiting verification.
    std::fs::write(&tmp_binary, b"unverified binary bytes").unwrap();

    // Port 1 on loopback refuses connections: the checksum download fails.
    let result = acquire_and_verify_checksum(
        "http://127.0.0.1:1/checksums-sha256.txt",
        &tmp_binary,
        &tmp_checksums,
        "keel-linux-amd64",
    );

    // Failure to obtain the checksum must be fatal.
    assert!(result.is_err(), "expected checksum acquisition to fail");
    assert!(
        result.unwrap_err().contains("unverified binary"),
        "error should signal refusal to install an unverified binary"
    );

    // Nothing was installed: the checksum file was never written and the
    // candidate binary is left untouched for the caller to discard.
    assert!(
        !tmp_checksums.exists(),
        "no checksum file should be written"
    );
    assert_eq!(
        std::fs::read(&tmp_binary).unwrap(),
        b"unverified binary bytes",
        "candidate binary must be untouched (not renamed/installed)"
    );
}

#[test]
fn checksum_mismatch_is_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let binary_path = dir.path().join("keel-test");
    let checksum_path = dir.path().join("checksums-sha256.txt");

    std::fs::write(&binary_path, b"real content").unwrap();
    std::fs::write(
        &checksum_path,
        "0000000000000000000000000000000000000000000000000000000000000000  keel-test\n",
    )
    .unwrap();

    // verify_checksum is the second half of acquire_and_verify_checksum and
    // must reject a tampered binary once the checksum file is present.
    let result = verify_checksum(&binary_path, &checksum_path, "keel-test");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("checksum mismatch"));
}

#[test]
fn verify_checksum_match() {
    let dir = tempfile::tempdir().unwrap();
    let binary_path = dir.path().join("keel-test");
    let checksum_path = dir.path().join("checksums-sha256.txt");

    let binary_content = b"hello world binary content";
    std::fs::write(&binary_path, binary_content).unwrap();

    // Compute the actual checksum
    let actual_hash = sha256_simple(binary_content).unwrap();

    // Write checksum file with the correct hash
    let checksum_content = format!("{actual_hash}  keel-test\n");
    std::fs::write(&checksum_path, checksum_content).unwrap();

    let result = verify_checksum(&binary_path, &checksum_path, "keel-test");
    assert!(result.is_ok(), "verify_checksum should pass: {:?}", result);
}

#[test]
fn verify_checksum_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let binary_path = dir.path().join("keel-test");
    let checksum_path = dir.path().join("checksums-sha256.txt");

    std::fs::write(&binary_path, b"real content").unwrap();
    std::fs::write(
        &checksum_path,
        "0000000000000000000000000000000000000000000000000000000000000000  keel-test\n",
    )
    .unwrap();

    let result = verify_checksum(&binary_path, &checksum_path, "keel-test");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("checksum mismatch"));
}

#[test]
fn verify_checksum_missing_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let binary_path = dir.path().join("keel-test");
    let checksum_path = dir.path().join("checksums-sha256.txt");

    std::fs::write(&binary_path, b"content").unwrap();
    std::fs::write(&checksum_path, "abcdef123456  other-artifact\n").unwrap();

    let result = verify_checksum(&binary_path, &checksum_path, "keel-test");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("no checksum found"));
}

#[test]
fn sync_keel_json_version_updates_pinned_version() {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    std::fs::create_dir_all(&keel_dir).unwrap();
    std::fs::write(
        keel_dir.join("keel.json"),
        r#"{"version": "0.3.6", "languages": []}"#,
    )
    .unwrap();

    sync_keel_json_version(dir.path(), "0.4.3");

    let content = std::fs::read_to_string(keel_dir.join("keel.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["version"], "0.4.3");
}

#[test]
fn sync_keel_json_version_is_a_noop_outside_an_initialized_project() {
    let dir = tempfile::tempdir().unwrap();
    // No `.keel/` at all — must not create one or panic.
    sync_keel_json_version(dir.path(), "0.4.3");
    assert!(!dir.path().join(".keel").exists());
}

#[test]
fn sha256_simple_known_hash() {
    // SHA-256 of empty string is well-known
    let hash = sha256_simple(b"").unwrap();
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
