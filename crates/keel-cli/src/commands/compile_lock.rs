use std::fs;
use std::path::Path;

/// Advisory lock guard for compile serialization.
/// Dropped automatically when the guard goes out of scope.
pub struct CompileLock {
    path: std::path::PathBuf,
}

impl Drop for CompileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Try to acquire a compile lock. Returns None if another compile holds the lock.
/// Uses a PID-based lockfile with stale lock detection.
pub fn acquire_compile_lock(keel_dir: &Path, verbose: bool) -> Option<CompileLock> {
    let lock_path = keel_dir.join("compile.lock");
    let pid = std::process::id();

    // Check for existing lock
    if lock_path.exists() {
        if let Ok(contents) = fs::read_to_string(&lock_path) {
            if let Ok(existing_pid) = contents.trim().parse::<u32>() {
                if is_process_alive(existing_pid) {
                    // Wait briefly (up to 2s) for the lock to release
                    for _ in 0..20 {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        if !lock_path.exists() {
                            break;
                        }
                    }
                    if lock_path.exists() {
                        return None; // Still locked
                    }
                } else if verbose {
                    eprintln!(
                        "keel compile: removing stale lock from PID {}",
                        existing_pid
                    );
                }
            }
        }
        // Stale lock or unreadable — remove it
        let _ = fs::remove_file(&lock_path);
    }

    // Write our PID
    if fs::write(&lock_path, pid.to_string()).is_err() {
        return None;
    }

    Some(CompileLock { path: lock_path })
}

/// Check if a process is still alive (cross-platform).
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Signal 0 checks if the process exists without sending a signal.
        // SAFETY: kill with signal 0 is a standard POSIX process existence check.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // Conservative fallback for Windows/other: assume the process is alive.
        // The 2-second wait loop will handle the timeout regardless.
        let _ = pid;
        true
    }
}
