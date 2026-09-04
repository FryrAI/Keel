use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

const COMPILE_LOCK_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Advisory graph lock guard shared by `keel compile` and `keel map`.
/// Dropped automatically when the guard goes out of scope.
pub struct CompileLock {
    path: std::path::PathBuf,
}

impl Drop for CompileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Try to acquire the graph lock, waiting up to two seconds for its holder.
/// Returns `None` if another keel process still holds the lock.
pub fn acquire_compile_lock(keel_dir: &Path, verbose: bool) -> Option<CompileLock> {
    acquire_compile_lock_with_timeout(keel_dir, verbose, COMPILE_LOCK_TIMEOUT)
}

/// Try to acquire the shared graph lock within `timeout`.
/// Uses a PID-based lockfile with atomic creation to avoid TOCTOU races.
pub(super) fn acquire_compile_lock_with_timeout(
    keel_dir: &Path,
    verbose: bool,
    timeout: Duration,
) -> Option<CompileLock> {
    let lock_path = keel_dir.join("compile.lock");
    let pid = std::process::id();
    let deadline = Instant::now() + timeout;

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
            // Wait for the lock to release, bounded by the caller's timeout.
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return None;
                }
                std::thread::sleep(LOCK_POLL_INTERVAL.min(remaining));
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
        }
        // Stale lock — process is dead
        if verbose {
            eprintln!("keel: removing stale graph lock from PID {}", existing_pid);
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
        // The caller's bounded wait loop will handle the timeout regardless.
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{acquire_compile_lock, acquire_compile_lock_with_timeout};

    /// The wait is bounded by the caller's timeout, and the lock is free again
    /// as soon as the holder drops it — `keel map` relies on both (#69).
    #[test]
    fn lock_wait_is_bounded_then_refused_while_held() {
        let dir = tempfile::tempdir().unwrap();
        let held = acquire_compile_lock(dir.path(), false).unwrap();
        let timeout = Duration::from_millis(25);
        let start = Instant::now();

        assert!(acquire_compile_lock_with_timeout(dir.path(), false, timeout).is_none());
        assert!(start.elapsed() >= timeout);

        drop(held);
        assert!(acquire_compile_lock_with_timeout(dir.path(), false, Duration::ZERO).is_some());
    }
}
