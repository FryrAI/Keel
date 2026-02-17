use std::fs;
use std::io::Read;
use std::path::PathBuf;

const REPO: &str = "FryrAI/Keel";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

enum InstallMethod {
    Homebrew,
    Cargo,
    Direct,
}

fn detect_install_method() -> InstallMethod {
    let exe = std::env::current_exe().unwrap_or_default();
    let path = exe.to_string_lossy();
    if path.contains("/Cellar/") || path.contains("/opt/homebrew/") {
        InstallMethod::Homebrew
    } else if path.contains("/.cargo/bin/") {
        InstallMethod::Cargo
    } else {
        InstallMethod::Direct
    }
}

fn platform_artifact() -> Result<String, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let platform = match os {
        "linux" => "linux",
        "macos" => "darwin",
        _ => return Err(format!(
            "unsupported OS: {os}. Download manually from https://github.com/{REPO}/releases"
        )),
    };

    let arch_suffix = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return Err(format!("unsupported architecture: {arch}")),
    };

    Ok(format!("keel-{platform}-{arch_suffix}"))
}

fn fetch_latest_version() -> Result<(String, String), String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut body = String::new();
    ureq::get(&url)
        .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("failed to check for updates: {e}"))?
        .into_body()
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("failed to read response: {e}"))?;

    let tag = extract_json_string(&body, "tag_name")
        .ok_or("could not parse version from GitHub API response")?;
    let version = tag.strip_prefix('v').unwrap_or(&tag).to_string();

    Ok((version, tag))
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

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    ureq::get(url)
        .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
        .call()
        .map_err(|e| format!("download failed: {e}"))?
        .into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("failed to read download: {e}"))?;
    Ok(bytes)
}

fn download_to(url: &str, dest: &PathBuf) -> Result<(), String> {
    let bytes = download_bytes(url)?;
    fs::write(dest, &bytes)
        .map_err(|e| format!("failed to write {}: {e}", dest.display()))?;
    Ok(())
}

fn verify_checksum(binary_path: &PathBuf, checksum_path: &PathBuf, artifact: &str) -> Result<(), String> {
    let checksums = fs::read_to_string(checksum_path)
        .map_err(|e| format!("failed to read checksums: {e}"))?;

    let expected = checksums
        .lines()
        .find(|line| line.contains(artifact))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| format!("no checksum found for {artifact}"))?;

    let binary_bytes = fs::read(binary_path)
        .map_err(|e| format!("failed to read binary: {e}"))?;

    let actual = sha256_simple(&binary_bytes);

    if expected != actual {
        return Err(format!(
            "checksum mismatch!\n  expected: {expected}\n  actual:   {actual}"
        ));
    }

    Ok(())
}

/// SHA-256 using command-line tool (avoids adding a crypto dependency)
fn sha256_simple(data: &[u8]) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .or_else(|_| {
            Command::new("shasum")
                .args(["-a", "256"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        })
        .unwrap_or_else(|_| panic!("neither sha256sum nor shasum found"));

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(data).ok();
    }

    let output = child.wait_with_output().expect("failed to wait for sha256sum");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split_whitespace().next().unwrap_or("").to_string()
}

pub fn run(version: Option<String>, yes: bool) -> i32 {
    match detect_install_method() {
        InstallMethod::Homebrew => {
            eprintln!("keel was installed via Homebrew. Update with:");
            eprintln!("  brew upgrade keel");
            return 0;
        }
        InstallMethod::Cargo => {
            eprintln!("keel was installed via cargo. Update with:");
            eprintln!("  cargo install keel-cli");
            return 0;
        }
        InstallMethod::Direct => {}
    }

    eprintln!("keel v{CURRENT_VERSION} — checking for updates...");

    let (latest_version, tag) = match version {
        Some(v) => {
            let tag = if v.starts_with('v') { v.clone() } else { format!("v{v}") };
            let ver = v.strip_prefix('v').unwrap_or(&v).to_string();
            (ver, tag)
        }
        None => match fetch_latest_version() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return 2;
            }
        },
    };

    if latest_version == CURRENT_VERSION {
        eprintln!("already at latest version (v{CURRENT_VERSION})");
        return 0;
    }

    let artifact = match platform_artifact() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };

    if !yes {
        eprintln!("upgrade keel v{CURRENT_VERSION} → v{latest_version}? [y/N] ");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err()
            || !input.trim().eq_ignore_ascii_case("y")
        {
            eprintln!("cancelled");
            return 0;
        }
    }

    let base_url = format!("https://github.com/{REPO}/releases/download/{tag}");
    let binary_url = format!("{base_url}/{artifact}");
    let checksum_url = format!("{base_url}/checksums-sha256.txt");

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current executable path: {e}");
            return 2;
        }
    };

    let tmp_binary = exe_path.with_extension("tmp");
    let tmp_checksums = exe_path.with_extension("checksums");

    eprintln!("downloading keel v{latest_version}...");
    if let Err(e) = download_to(&binary_url, &tmp_binary) {
        eprintln!("error: {e}");
        let _ = fs::remove_file(&tmp_binary);
        return 2;
    }

    eprintln!("verifying checksum...");
    if let Err(e) = download_to(&checksum_url, &tmp_checksums) {
        eprintln!("warning: could not download checksums: {e}");
    } else if let Err(e) = verify_checksum(&tmp_binary, &tmp_checksums, &artifact) {
        eprintln!("error: {e}");
        let _ = fs::remove_file(&tmp_binary);
        let _ = fs::remove_file(&tmp_checksums);
        return 2;
    }
    let _ = fs::remove_file(&tmp_checksums);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_binary, fs::Permissions::from_mode(0o755));
    }

    if let Err(e) = fs::rename(&tmp_binary, &exe_path) {
        eprintln!("error: failed to replace binary: {e}");
        eprintln!("try: sudo mv {} {}", tmp_binary.display(), exe_path.display());
        return 2;
    }

    eprintln!("upgraded to keel v{latest_version}");
    0
}
