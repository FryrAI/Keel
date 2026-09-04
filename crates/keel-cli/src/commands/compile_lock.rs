use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

const COMPILE_LOCK_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);
const UNPARSEABLE_LOCK_STALE_AFTER: Duration = Duration::from_secs(30);

/// Advisory graph lock guard shared by `keel compile` and `keel map`.
/// Dropped automatically when the guard goes out of scope.
pub struct CompileLock {
    path: std::path::PathBuf,
}

impl Drop for CompileLock {
    fn drop(&mut self) {
        let owned = fs::read_to_string(&self.path)
            .ok()
            .and_then(|contents| contents.trim().parse::<u32>().ok())
            == Some(std::process::id());
        if owned {
            let _ = fs::remove_file(&self.path);
        }
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

    loop {
        // Atomic creation remains the only way to obtain the lock.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let _ = write!(file, "{}", pid);
                return Some(CompileLock { path: lock_path });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }

        let existing_pid = fs::read_to_string(&lock_path)
            .ok()
            .and_then(|contents| contents.trim().parse::<u32>().ok());
        let stale = match existing_pid {
            Some(existing_pid) => !is_process_alive(existing_pid),
            None => fs::metadata(&lock_path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > UNPARSEABLE_LOCK_STALE_AFTER),
        };

        if stale {
            let stale_path = keel_dir.join(format!("compile.lock.stale.{pid}"));
            if fs::rename(&lock_path, &stale_path).is_ok() {
                if verbose {
                    match existing_pid {
                        Some(existing_pid) => {
                            eprintln!("keel: removing stale graph lock from PID {existing_pid}");
                        }
                        None => eprintln!("keel: removing stale unreadable graph lock"),
                    }
                }
                let _ = fs::remove_file(stale_path);
                continue;
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        std::thread::sleep(LOCK_POLL_INTERVAL.min(remaining));
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
    use std::fs;
    use std::time::{Duration, Instant, SystemTime};

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

    #[test]
    fn fresh_empty_lock_is_treated_as_held() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("compile.lock");
        fs::File::create(&lock_path).unwrap();

        assert!(
            acquire_compile_lock_with_timeout(dir.path(), false, Duration::from_millis(50))
                .is_none()
        );
        assert!(lock_path.exists());
    }

    #[test]
    fn old_empty_lock_is_reclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("compile.lock");
        let lock_file = fs::File::create(&lock_path).unwrap();
        lock_file
            .set_modified(SystemTime::now() - Duration::from_secs(31))
            .unwrap();

        let held = acquire_compile_lock_with_timeout(dir.path(), false, Duration::ZERO).unwrap();
        assert_eq!(
            fs::read_to_string(&lock_path).unwrap(),
            std::process::id().to_string()
        );
        drop(held);
    }

    #[cfg(unix)]
    #[test]
    fn dead_pid_lock_is_reclaimed_via_rename() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("compile.lock");
        // Above any pid_max and still a positive pid_t: a negative cast would
        // turn the liveness probe into a process-group query.
        let dead_pid = i32::MAX as u32;
        assert!(!super::is_process_alive(dead_pid));
        fs::write(&lock_path, dead_pid.to_string()).unwrap();

        let held = acquire_compile_lock_with_timeout(dir.path(), false, Duration::ZERO).unwrap();
        assert_eq!(
            fs::read_to_string(&lock_path).unwrap(),
            std::process::id().to_string()
        );
        assert!(!dir
            .path()
            .join(format!("compile.lock.stale.{}", std::process::id()))
            .exists());
        drop(held);
    }

    #[test]
    fn drop_preserves_lock_owned_by_another_pid() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("compile.lock");
        let held = acquire_compile_lock(dir.path(), false).unwrap();
        let other_pid = std::process::id().checked_add(1).unwrap_or(1);
        fs::write(&lock_path, other_pid.to_string()).unwrap();

        drop(held);

        assert_eq!(
            fs::read_to_string(lock_path).unwrap(),
            other_pid.to_string()
        );
    }
}
