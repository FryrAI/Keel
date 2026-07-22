use std::process::Command;

/// Run `git` with `args` and return trimmed stdout on success.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Embed the git short SHA into the version string (`0.4.3 (abc123def)`) so an
/// unreleased dev build can never masquerade as a released version (#48).
/// Falls back to the plain crate version when there is no git context
/// (e.g. a crates.io build).
fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let full = match git(&["rev-parse", "--short=9", "HEAD"]) {
        Some(sha) => format!("{version} ({sha})"),
        None => version,
    };
    println!("cargo:rustc-env=KEEL_VERSION_FULL={full}");

    // Re-embed when HEAD moves; skip missing ref files (packed refs).
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            let ref_path = format!("{dir}/{head_ref}");
            if std::path::Path::new(&ref_path).exists() {
                println!("cargo:rerun-if-changed={ref_path}");
            }
        }
    }
}
