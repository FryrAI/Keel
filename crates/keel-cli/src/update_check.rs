//! Passive update check — notifies users when a newer keel version is available.
//!
//! Runs at most once per 24 hours, never blocks CLI execution, and respects
//! `--no-telemetry` and CI environments.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::keel_home;

const CHECK_FILE: &str = "update-check";
const CHECK_INTERVAL_SECS: u64 = 86_400; // 24 hours
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "FryrAI/Keel";

/// Cached update check state, persisted as JSON in `~/.keel/update-check`.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct UpdateCheck {
    last_checked: u64,
    latest_version: Option<String>,
}

fn check_path() -> Option<PathBuf> {
    keel_home().map(|d| d.join(CHECK_FILE))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_check() -> Option<UpdateCheck> {
    let data = fs::read_to_string(check_path()?).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_check(state: &UpdateCheck) {
    if let Some(path) = check_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, serde_json::to_string(state).unwrap_or_default());
    }
}

/// Returns true if `candidate` is a newer semver than `current`.
fn is_newer(current: &str, candidate: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
    let cur = parse(current);
    let cand = parse(candidate);
    for i in 0..3 {
        let c = cur.get(i).copied().unwrap_or(0);
        let n = cand.get(i).copied().unwrap_or(0);
        if n > c {
            return true;
        }
        if n < c {
            return false;
        }
    }
    false
}

/// Print a one-line notification to stderr if a newer version is known.
/// Called early in CLI startup — never fails, never blocks.
pub fn maybe_notify() {
    if let Some(state) = read_check() {
        if let Some(ref latest) = state.latest_version {
            if is_newer(CURRENT_VERSION, latest) {
                eprintln!(
                    "keel v{latest} available (current: v{CURRENT_VERSION}). \
                     Run 'keel upgrade' to update."
                );
            }
        }
    }
}

/// If the last check was >24h ago, spawn a background thread to query GitHub
/// for the latest release and update the check file. Skipped when
/// `no_telemetry` is set or the `CI` env var is present.
pub fn maybe_check_async(no_telemetry: bool) {
    if no_telemetry || std::env::var("CI").is_ok() {
        return;
    }

    let stale = match read_check() {
        Some(state) => now_secs().saturating_sub(state.last_checked) >= CHECK_INTERVAL_SECS,
        None => true,
    };
    if !stale {
        return;
    }

    std::thread::spawn(|| {
        if let Ok(version) = fetch_latest_tag() {
            write_check(&UpdateCheck {
                last_checked: now_secs(),
                latest_version: Some(version),
            });
        } else {
            // Update timestamp even on failure so we don't hammer the API.
            write_check(&UpdateCheck {
                last_checked: now_secs(),
                latest_version: read_check().and_then(|s| s.latest_version),
            });
        }
    });
}

/// Fetch the latest release tag from GitHub. Returns the version without the
/// leading `v` (e.g. `"0.4.0"`).
fn fetch_latest_tag() -> Result<String, String> {
    use std::io::Read;
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut body = String::new();
    ureq::get(&url)
        .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("update check failed: {e}"))?
        .into_body()
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("failed to read response: {e}"))?;

    let tag = crate::commands::json_helpers::extract_json_string(&body, "tag_name")
        .ok_or("could not parse version from GitHub API")?;
    Ok(tag.strip_prefix('v').unwrap_or(&tag).to_string())
}

#[cfg(test)]
#[path = "update_check_tests.rs"]
mod tests;
