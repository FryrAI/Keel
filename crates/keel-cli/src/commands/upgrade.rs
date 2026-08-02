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
    artifact_name(std::env::consts::OS, std::env::consts::ARCH)
}

/// Map an OS/arch pair to the release artifact filename published by the
/// release workflow (see `.github/workflows/release.yml`). Windows artifacts
/// carry a `.exe` suffix.
fn artifact_name(os: &str, arch: &str) -> Result<String, String> {
    let platform = match os {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        _ => {
            return Err(format!(
                "unsupported OS: {os}. Download manually from https://github.com/{REPO}/releases"
            ))
        }
    };

    let arch_suffix = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return Err(format!("unsupported architecture: {arch}")),
    };

    let ext = if os == "windows" { ".exe" } else { "" };
    Ok(format!("keel-{platform}-{arch_suffix}{ext}"))
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

use super::json_helpers::extract_json_string;

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;

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
    fs::write(dest, &bytes).map_err(|e| format!("failed to write {}: {e}", dest.display()))?;
    Ok(())
}

fn verify_checksum(
    binary_path: &PathBuf,
    checksum_path: &PathBuf,
    artifact: &str,
) -> Result<(), String> {
    let checksums =
        fs::read_to_string(checksum_path).map_err(|e| format!("failed to read checksums: {e}"))?;

    let expected = checksums
        .lines()
        .find(|line| line.contains(artifact))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| format!("no checksum found for {artifact}"))?;

    let binary_bytes = fs::read(binary_path).map_err(|e| format!("failed to read binary: {e}"))?;

    let actual = sha256_simple(&binary_bytes)?;

    if expected != actual {
        return Err(format!(
            "checksum mismatch!\n  expected: {expected}\n  actual:   {actual}"
        ));
    }

    Ok(())
}

/// Download the checksum manifest and verify the freshly downloaded binary
/// against it. A missing or failed checksum download is fatal: keel refuses to
/// install a binary it cannot verify. This helper never touches the installed
/// executable, so an `Err` guarantees nothing was installed.
fn acquire_and_verify_checksum(
    checksum_url: &str,
    tmp_binary: &PathBuf,
    tmp_checksums: &PathBuf,
    artifact: &str,
) -> Result<(), String> {
    download_to(checksum_url, tmp_checksums).map_err(|e| {
        format!("could not download checksums; refusing to install an unverified binary: {e}")
    })?;
    verify_checksum(tmp_binary, tmp_checksums, artifact)
}

/// SHA-256 using command-line tool (avoids adding a crypto dependency).
fn sha256_simple(data: &[u8]) -> Result<String, String> {
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
        .map_err(|_| "neither sha256sum nor shasum found".to_string())?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(data).ok();
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for sha256sum: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.split_whitespace().next().unwrap_or("").to_string())
}

/// Run `keel upgrade` -- download and install the latest (or specified) keel release binary.
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
            let tag = if v.starts_with('v') {
                v.clone()
            } else {
                format!("v{v}")
            };
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
    if let Err(e) =
        acquire_and_verify_checksum(&checksum_url, &tmp_binary, &tmp_checksums, &artifact)
    {
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

    if let Err(e) = replace_executable(&tmp_binary, &exe_path) {
        eprintln!("error: {e}");
        eprintln!(
            "try: sudo mv {} {}",
            tmp_binary.display(),
            exe_path.display()
        );
        let _ = fs::remove_file(&tmp_binary);
        return 2;
    }

    eprintln!("upgraded to keel v{latest_version}");
    if let Ok(cwd) = std::env::current_dir() {
        sync_keel_json_version(&cwd, &latest_version);
    }
    0
}

/// Sync `.keel/keel.json`'s pinned version to the binary just installed, if
/// `keel upgrade` happened to be run from inside an initialized project.
/// Best-effort and silent on failure: the upgrade itself already succeeded,
/// and a project outside any `.keel/` (or whose config can't be written) is
/// not an upgrade error. This never touches the generated docs — those stay
/// stale until the human runs `keel init --update-docs`, which `map`/`compile`
/// now point at (Principle 7: never auto-rewrite user files).
fn sync_keel_json_version(cwd: &std::path::Path, version: &str) {
    let keel_dir = keel_core::paths::keel_dir(cwd);
    let _ = keel_core::config::KeelConfig::sync_version(&keel_dir, version);
}

/// Move `new_binary` into place at `exe_path`, replacing the running executable.
///
/// On Unix a plain `rename` atomically swaps the inode out from under the
/// running process. On Windows the OS refuses to overwrite a running `.exe`, so
/// the current executable is first renamed aside to `keel.exe.old` (permitted
/// even while running) before the new binary is moved in. The stale `.old` file
/// cannot be deleted while the process is live; it is cleaned up best-effort by
/// the next upgrade.
#[cfg(not(windows))]
fn replace_executable(new_binary: &PathBuf, exe_path: &PathBuf) -> Result<(), String> {
    fs::rename(new_binary, exe_path).map_err(|e| format!("failed to replace binary: {e}"))
}

#[cfg(windows)]
fn replace_executable(new_binary: &PathBuf, exe_path: &PathBuf) -> Result<(), String> {
    let mut old_os = exe_path.clone().into_os_string();
    old_os.push(".old");
    let old_path = PathBuf::from(old_os);

    // Best-effort removal of a leftover .old from a previous upgrade.
    let _ = fs::remove_file(&old_path);

    fs::rename(exe_path, &old_path)
        .map_err(|e| format!("failed to move current binary aside: {e}"))?;

    if let Err(e) = fs::rename(new_binary, exe_path) {
        // Restore the original so the install is not left broken.
        let _ = fs::rename(&old_path, exe_path);
        return Err(format!("failed to replace binary: {e}"));
    }

    // The running process still has the old image mapped; this usually fails
    // and is retried on the next upgrade.
    let _ = fs::remove_file(&old_path);
    Ok(())
}
