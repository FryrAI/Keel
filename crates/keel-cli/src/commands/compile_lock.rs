use std::fs::{self, OpenOptions};
use std::io::Write;
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
/// Uses a PID-based lockfile with atomic creation to avoid TOCTOU races.
pub fn acquire_compile_lock(keel_dir: &Path, verbose: bool) -> Option<CompileLock> {
    let lock_path = keel_dir.join("compile.lock");
    let pid = std::process::id();

    // Try atomic create — fails if file already exists
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut file) => {
            let _ = write!(file, "{}", pid);
            return Some(CompileLock { path: lock_path });
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Lock file exists — check if holder is still alive
        }
        Err(_) => return None,
    }

    // Read the existing lock's PID
    let existing_pid = fs::read_to_string(&lock_path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    if let Some(existing_pid) = existing_pid {
        if is_process_alive(existing_pid) {
            // Wait up to 2s for the lock to release, then retry once
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)
                {
                    Ok(mut file) => {
                        let _ = write!(file, "{}", pid);
                        return Some(CompileLock { path: lock_path });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(_) => return None,
                }
            }
            return None; // Still locked after 2s
        }
        // Stale lock — process is dead
        if verbose {
            eprintln!(
                "keel compile: removing stale lock from PID {}",
                existing_pid
            );
        }
    }

    // Stale or unreadable lock — remove and retry once
    let _ = fs::remove_file(&lock_path);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut file) => {
            let _ = write!(file, "{}", pid);
            Some(CompileLock { path: lock_path })
        }
        Err(_) => None,
    }
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
