use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

/// Run a known agent hook: the script of the nearest directory, from the
/// current one up to the project root, that holds `.keel/hooks/<name>.sh`.
/// Post-edit reads and forwards stdin, running that script in the edited file's
/// checkout when it is another keel-initialised checkout, and naming it on failure.
pub fn run(name: &str, args: &[OsString]) -> i32 {
    if !matches!(name, "post-edit" | "plan-check" | "session-start") {
        eprintln!("keel hook: unknown hook {name:?}");
        return 2;
    }
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("keel hook: failed to get current directory: {e}");
            return 1;
        }
    };
    let root = keel_core::paths::project_root(&cwd);
    let (script, dir) = hook_script(&cwd, &root, name);
    if !script.try_exists().unwrap_or(true) {
        let local = root.join(".keel").join(format!("hooks/{name}.sh"));
        eprintln!("keel hook: script not found: {}", local.display());
        return 1;
    }
    let payload = if name == "post-edit" {
        let mut bytes = Vec::new();
        if let Err(e) = io::stdin().read_to_end(&mut bytes) {
            eprintln!("keel hook: failed to read stdin: {e}");
            return 1;
        }
        Some(bytes)
    } else {
        None
    };
    let edited_root = payload
        .as_deref()
        .and_then(|bytes| edited_checkout(bytes, &root));
    // Hook scripts are tracked files: editing a freshly cloned repository must
    // not execute its script. Only the working directory comes from the payload.
    match run_script(
        &script,
        edited_root.as_deref().unwrap_or(&dir),
        args,
        payload.as_deref(),
        edited_root.is_some(),
    ) {
        Ok(status) => {
            let code = status.code().unwrap_or_else(|| {
                eprintln!("keel hook: script terminated by a signal");
                1
            });
            if let Some(root) = edited_root.filter(|_| code != 0) {
                eprintln!("keel hook: checked in {} (the edited file's checkout) — run keel commands for this file from that directory", root.display());
            }
            code
        }
        Err(e) => {
            eprintln!("keel hook: failed to start {}: {e}", script.display());
            1
        }
    }
}

/// Find another keel-initialised checkout from an absolute edited-file path.
fn edited_checkout(payload: &[u8], root: &Path) -> Option<PathBuf> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let file = Path::new(value.get("tool_input")?.get("file_path")?.as_str()?);
    if !file.is_absolute() {
        return None;
    }
    let parent = keel_core::paths::canonicalize_portable(file.parent()?).ok()?;
    let edited_root = keel_core::paths::project_root(&parent);
    let root = keel_core::paths::canonicalize_portable(root).ok()?;
    // `keel.json`, not a bare `.keel`: keel's user-level state lives in `~/.keel`.
    let initialised = keel_core::paths::keel_dir(&edited_root)
        .join("keel.json")
        .exists();
    (edited_root != root && initialised).then_some(edited_root)
}

fn hook_script(cwd: &Path, root: &Path, name: &str) -> (PathBuf, PathBuf) {
    let relative = format!("hooks/{name}.sh");
    // `try_exists`: an unreadable hooks directory counts as present, so the
    // spawn fails loudly instead of silently running another checkout's copy.
    // Portable form: a plain canonicalize adds a Windows verbatim prefix the root lacks.
    let nearest = keel_core::paths::canonicalize_portable(cwd)
        .unwrap_or_else(|_| cwd.to_path_buf())
        .ancestors()
        .take_while(|dir| dir.starts_with(root))
        .find(|dir| {
            dir.join(".keel")
                .join(&relative)
                .try_exists()
                .unwrap_or(true)
        })
        .map(Path::to_path_buf);
    match nearest {
        Some(dir) => (dir.join(".keel").join(&relative), dir),
        None => (
            keel_core::paths::keel_dir(root).join(relative),
            root.to_path_buf(),
        ),
    }
}

fn run_script(
    script: &Path,
    dir: &Path,
    args: &[OsString],
    payload: Option<&[u8]>,
    other_checkout: bool,
) -> io::Result<ExitStatus> {
    // On Windows, `sh` resolves through PATH; a bare `bash` finds the WSL launcher.
    // Let `sh` exec the script so its shebang selects the interpreter.
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "exec \"$0\" \"$@\""]).arg(script);
        command
    };
    #[cfg(not(windows))]
    let mut command = Command::new(script);
    command.args(args).current_dir(dir);
    // A relative `PATH` entry would resolve to files of the other checkout.
    if let Some(path) = std::env::var_os("PATH").filter(|_| other_checkout) {
        let absolute = std::env::split_paths(&path).filter(|entry| entry.is_absolute());
        if let Ok(path) = std::env::join_paths(absolute) {
            command.env("PATH", path);
        }
    }
    if let Some(bytes) = payload {
        let mut child = command.stdin(Stdio::piped()).spawn()?;
        if let Err(e) = child.stdin.take().expect("piped stdin").write_all(bytes) {
            if e.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("keel hook: failed to write script stdin: {e}");
            }
        }
        child.wait()
    } else {
        command.status()
    }
}
