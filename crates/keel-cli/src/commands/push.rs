//! `keel push` — upload graph database to keel cloud.

use std::io::Read;
use std::path::Path;

use crate::auth;

const API_BASE: &str = "https://api.keel.engineer";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_UPLOAD_SIZE: u64 = 100 * 1024 * 1024; // 100MB

/// Run the push command.
pub fn run(
    formatter: &dyn keel_output::OutputFormatter,
    verbose: bool,
    yes: bool,
) -> i32 {
    let _ = formatter; // reserved for future structured output

    // Require authentication
    let creds = match auth::load_credentials() {
        Some(c) if !c.is_expired() => c,
        Some(_) => {
            eprintln!("error: session expired. Run `keel login` to re-authenticate.");
            return 2;
        }
        None => {
            eprintln!("error: not logged in. Run `keel login` first.");
            return 2;
        }
    };

    // Locate graph.db
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let keel_dir = cwd.join(".keel");
    let graph_path = keel_dir.join("graph.db");

    if !graph_path.exists() {
        eprintln!("error: no graph.db found. Run `keel map` first.");
        return 2;
    }

    let file_size = match std::fs::metadata(&graph_path) {
        Ok(m) => m.len(),
        Err(e) => {
            eprintln!("error: cannot read graph.db: {e}");
            return 2;
        }
    };

    if file_size > MAX_UPLOAD_SIZE {
        eprintln!(
            "error: graph.db is {}MB (max {}MB)",
            file_size / (1024 * 1024),
            MAX_UPLOAD_SIZE / (1024 * 1024)
        );
        return 2;
    }

    // Read project_id from .keel/keel.json if it exists
    let project_id = read_project_id(&keel_dir);
    let commit_sha = detect_commit_sha();

    // Confirmation prompt
    if !yes {
        let target = project_id
            .as_deref()
            .unwrap_or("new project");
        eprintln!(
            "push graph.db ({:.1}MB) to {target}? [y/N] ",
            file_size as f64 / (1024.0 * 1024.0)
        );
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err()
            || !input.trim().eq_ignore_ascii_case("y")
        {
            eprintln!("cancelled");
            return 0;
        }
    }

    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .build(),
    );

    // Check sync status (determines full vs incremental)
    if let Some(ref pid) = project_id {
        if let Some(server_hash) = check_sync_status(&agent, &creds, pid, verbose) {
            if Some(server_hash.as_str()) == commit_sha.as_deref() {
                eprintln!("graph is already up to date.");
                return 0;
            }
            // TODO: implement incremental diff via PATCH
            // For now, always do full upload
            if verbose {
                eprintln!("server has previous push, doing full upload (incremental not yet implemented)");
            }
        }
    }

    // Full upload
    eprintln!("uploading graph.db...");
    match upload_full(&agent, &creds, &graph_path, project_id.as_deref(), &commit_sha) {
        Ok(new_project_id) => {
            // Store project_id if we got a new one
            if project_id.is_none() {
                if let Some(ref pid) = new_project_id {
                    save_project_id(&keel_dir, pid);
                }
            }
            eprintln!("push complete.");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            2
        }
    }
}

fn check_sync_status(
    agent: &ureq::Agent,
    creds: &auth::Credentials,
    project_id: &str,
    verbose: bool,
) -> Option<String> {
    let url = format!("{API_BASE}/projects/{project_id}/sync-status");
    let resp = agent
        .get(&url)
        .header("Authorization", &format!("Bearer {}", creds.access_token))
        .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
        .call()
        .ok()?;

    let mut body = String::new();
    resp.into_body().into_reader().read_to_string(&mut body).ok()?;

    if verbose {
        eprintln!("sync-status: {body}");
    }

    extract_json_string(&body, "last_push_hash")
}

fn upload_full(
    agent: &ureq::Agent,
    creds: &auth::Credentials,
    graph_path: &Path,
    project_id: Option<&str>,
    commit_sha: &Option<String>,
) -> Result<Option<String>, String> {
    let pid = project_id.unwrap_or("new");
    let url = format!("{API_BASE}/projects/{pid}/graph");
    let graph_bytes =
        std::fs::read(graph_path).map_err(|e| format!("cannot read graph.db: {e}"))?;

    // Build multipart-style JSON metadata + raw upload
    // Server expects multipart but we send raw bytes with metadata headers
    let mut resp_body = String::new();
    agent
        .post(&url)
        .header("Authorization", &format!("Bearer {}", creds.access_token))
        .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
        .header("Content-Type", "application/octet-stream")
        .header("X-Keel-Version", CURRENT_VERSION)
        .header(
            "X-Commit-SHA",
            commit_sha.as_deref().unwrap_or("unknown"),
        )
        .send(graph_bytes.as_slice())
        .map_err(|e| format!("upload failed: {e}"))?
        .into_body()
        .into_reader()
        .read_to_string(&mut resp_body)
        .map_err(|e| format!("failed to read upload response: {e}"))?;

    Ok(extract_json_string(&resp_body, "project_id"))
}

fn detect_commit_sha() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn read_project_id(keel_dir: &Path) -> Option<String> {
    let config_path = keel_dir.join("keel.json");
    let data = std::fs::read_to_string(config_path).ok()?;
    extract_json_string(&data, "project_id")
}

fn save_project_id(keel_dir: &Path, project_id: &str) {
    let config_path = keel_dir.join("keel.json");

    // Read existing config or start fresh
    let mut content = std::fs::read_to_string(&config_path).unwrap_or_else(|_| "{}".into());

    // Simple JSON injection — insert project_id
    if content.contains("\"project_id\"") {
        // Already has one, leave it
        return;
    }
    if content.trim() == "{}" {
        content = format!(r#"{{"project_id":"{project_id}"}}"#);
    } else {
        // Insert after opening brace
        content = content.replacen('{', &format!(r#"{{"project_id":"{project_id}","#), 1);
        // Fix double opening brace
        content = content.replace("{{", "{");
    }

    let _ = std::fs::write(config_path, content);
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
