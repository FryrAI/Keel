use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// Run a known agent hook: the script of the nearest directory, from the
/// current one up to the project root, that holds `.keel/hooks/<name>.sh`.
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
    let relative = format!("hooks/{name}.sh");
    // `try_exists`: an unreadable hooks directory counts as present, so the
    // spawn fails loudly instead of silently running another checkout's copy.
    // Portable form: a plain canonicalize adds a Windows verbatim prefix the root lacks.
    let nearest = keel_core::paths::canonicalize_portable(&cwd)
        .unwrap_or(cwd)
        .ancestors()
        .take_while(|dir| dir.starts_with(&root))
        .find(|dir| {
            dir.join(".keel")
                .join(&relative)
                .try_exists()
                .unwrap_or(true)
        })
        .map(Path::to_path_buf);
    let local = root.join(".keel").join(&relative);
    let (script, dir) = match nearest {
        Some(dir) => (dir.join(".keel").join(&relative), dir),
        None => (keel_core::paths::keel_dir(&root).join(relative), root),
    };
    if !script.try_exists().unwrap_or(true) {
        eprintln!("keel hook: script not found: {}", local.display());
        return 1;
    }
    // On Windows, `sh` resolves through PATH; a bare `bash` finds the WSL launcher.
    // Let `sh` exec the script so its shebang selects the interpreter.
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "exec \"$0\" \"$@\""]).arg(&script);
        command
    };
    #[cfg(not(windows))]
    let mut command = Command::new(&script);
    match command.args(args).current_dir(dir).status() {
        Ok(status) => status.code().unwrap_or_else(|| {
            eprintln!("keel hook: script terminated by a signal");
            1
        }),
        Err(e) => {
            eprintln!("keel hook: failed to start {}: {e}", script.display());
            1
        }
    }
}
