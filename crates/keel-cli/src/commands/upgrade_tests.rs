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
fn sha256_simple_known_hash() {
    // SHA-256 of empty string is well-known
    let hash = sha256_simple(b"").unwrap();
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
