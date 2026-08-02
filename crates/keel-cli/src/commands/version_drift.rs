//! Version-drift detection between the running binary, `.keel/keel.json`'s
//! pinned version, and the `<!-- keel:version X -->` stamp `keel init`
//! writes into generated agent docs (see `init::templates`).
//!
//! `map`, `compile` and `review` call [`warn`] once per invocation, which
//! prints the diagnostic (if any) to stderr. keel only *detects* drift here —
//! it never rewrites a file on its own; `keel init --update-docs` is the
//! human-authorized fix (Principle 7: never auto-rewrite user files).

use std::path::Path;

use keel_core::config::KeelConfig;

/// Print the version-drift diagnostic for this project to stderr, if there is
/// one. The single call site shape for every command that reports drift.
pub(crate) fn warn(cwd: &Path, config: &KeelConfig) {
    if let Some(msg) = version_drift_message(cwd, config) {
        eprintln!("{msg}");
    }
}

/// Compute the one-line version-drift diagnostic `map`/`compile`/`review`
/// print at most once per invocation, or `None` when the binary matches both
/// `.keel/keel.json`'s pinned version and the generated docs' stamp.
///
/// Takes the already-loaded config: every caller needs it anyway a few lines
/// later, and re-reading `keel.json` here would just be a second parse of the
/// same file.
///
/// Checks `.keel/keel.json` first since it is the authoritative "what
/// version was this project set up with" record; if that already matches the
/// running binary (e.g. `keel upgrade` just synced it — see
/// `commands::upgrade::sync_keel_json_version`) but the generated `AGENTS.md`
/// still carries an older stamp, that is reported instead. A project with no
/// stamped `AGENTS.md` (never had `keel init` run past this change, or the
/// file was removed) is not treated as drift beyond the `keel.json` check.
pub(crate) fn version_drift_message(cwd: &Path, config: &KeelConfig) -> Option<String> {
    let binary_version = env!("CARGO_PKG_VERSION");
    let config_version = &config.version;
    if config_version != binary_version {
        return Some(format!(
            "keel: .keel/keel.json records {config_version}, binary is {binary_version} — run keel init --update-docs"
        ));
    }

    let stamp = docs_version_stamp(cwd)?;
    if stamp != binary_version {
        return Some(format!(
            "keel: AGENTS.md records {stamp}, binary is {binary_version} — run keel init --update-docs"
        ));
    }
    None
}

/// Extract the `<!-- keel:version X -->` stamp from the project's generated
/// `AGENTS.md`, or `None` if the file or the stamp is missing.
fn docs_version_stamp(cwd: &Path) -> Option<String> {
    let content = std::fs::read_to_string(cwd.join("AGENTS.md")).ok()?;
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix("<!-- keel:version ")?
            .strip_suffix(" -->")
            .map(str::to_string)
    })
}

#[cfg(test)]
#[path = "version_drift_tests.rs"]
mod tests;
