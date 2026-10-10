use std::ffi::OsString;
use std::process::Command;

/// Run a known agent hook at the current worktree's project root.
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
    let local = root.join(".keel").join(&relative);
    let script = if local.exists() {
        local.clone()
    } else {
        keel_core::paths::keel_dir(&root).join(relative)
    };
    if !script.exists() {
        eprintln!("keel hook: script not found: {}", local.display());
        return 1;
    }
    match Command::new(&script).args(args).current_dir(root).status() {
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
