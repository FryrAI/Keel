//! Global credential management for keel authentication.
//!
//! Credentials are stored in `~/.keel/credentials.json` (global, not per-project).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const CREDENTIALS_FILE: &str = "credentials.json";

/// Stored authentication credentials from `keel login`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: u64,
}

impl Credentials {
    /// Returns true if the access token has expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at
    }
}

/// Returns the global keel home directory (`~/.keel`).
/// Uses `$HOME` on Unix, `%USERPROFILE%` on Windows.
pub fn keel_home() -> Option<PathBuf> {
    #[cfg(unix)]
    let home = std::env::var("HOME").ok();
    #[cfg(windows)]
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok());
    #[cfg(not(any(unix, windows)))]
    let home = std::env::var("HOME").ok();

    home.map(|h| PathBuf::from(h).join(".keel"))
}

/// Load credentials from `~/.keel/credentials.json`.
/// Returns `None` if the file doesn't exist or can't be parsed.
pub fn load_credentials() -> Option<Credentials> {
    let path = keel_home()?.join(CREDENTIALS_FILE);
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Save credentials to `~/.keel/credentials.json`.
/// Creates the directory if it doesn't exist.
pub fn save_credentials(creds: &Credentials) -> Result<(), String> {
    let dir = keel_home().ok_or("cannot determine home directory")?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let path = dir.join(CREDENTIALS_FILE);
    let json = serde_json::to_string_pretty(creds)
        .map_err(|e| format!("failed to serialize credentials: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Remove stored credentials.
pub fn clear_credentials() {
    if let Some(path) = keel_home().map(|d| d.join(CREDENTIALS_FILE)) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
