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
    let version = env!("CARGO_PKG_VERSION");
    let full = match git(&["rev-parse", "--short=9", "HEAD"]) {
        Some(sha) => format!("{version} ({sha})"),
        None => version.to_string(),
    };
    println!("cargo:rustc-env=KEEL_VERSION_FULL={full}");

    // Re-embed when HEAD moves. The reflog is appended on every commit and
    // checkout even when refs are packed; loose paths are skipped if missing
    // so cargo doesn't treat them as perpetually changed.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        for path in [format!("{dir}/HEAD"), format!("{dir}/logs/HEAD")]
            .into_iter()
            .chain(git(&["symbolic-ref", "-q", "HEAD"]).map(|head_ref| format!("{dir}/{head_ref}")))
        {
            if std::path::Path::new(&path).exists() {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
}
