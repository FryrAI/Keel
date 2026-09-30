use super::*;

#[test]
fn second_handle_is_busy_and_drop_releases_without_deleting() {
    let dir = tempfile::tempdir().unwrap();
    let held = try_acquire(dir.path()).unwrap();
    assert!(matches!(try_acquire(dir.path()), Err(GraphLockError::Busy)));
    drop(held);
    // Contention never truncates the holder's diagnostics.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("compile.lock")).unwrap(),
        std::process::id().to_string()
    );
    assert!(dir.path().join("compile.lock").exists());
    assert!(try_acquire(dir.path()).is_ok());
}

#[test]
fn nonexistent_pid_and_garbage_are_acquired_immediately() {
    let dir = tempfile::tempdir().unwrap();
    for contents in ["4294967294", "garbage", ""] {
        std::fs::write(dir.path().join("compile.lock"), contents).unwrap();
        let held = acquire(dir.path(), Duration::ZERO).unwrap();
        drop(held);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("compile.lock")).unwrap(),
            std::process::id().to_string()
        );
    }
}

#[test]
fn lock_wait_is_bounded_then_refused_while_held() {
    let dir = tempfile::tempdir().unwrap();
    let _held = try_acquire(dir.path()).unwrap();
    let timeout = Duration::from_millis(25);
    let start = Instant::now();
    assert!(matches!(
        acquire(dir.path(), timeout),
        Err(GraphLockError::Busy)
    ));
    assert!(start.elapsed() >= timeout);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn missing_directory_and_non_directory_surface_io_errors() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    for path in [file, dir.path().join("missing")] {
        assert!(matches!(
            acquire(&path, Duration::ZERO),
            Err(GraphLockError::Io(_))
        ));
    }
}

#[test]
fn child_lock_holder() {
    let Some(dir) = std::env::var_os("KEEL_GRAPH_LOCK_CHILD") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let _held = try_acquire(&dir).unwrap();
    std::fs::write(dir.join("ready"), "").unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn killed_holder_releases_kernel_lock() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["graph_lock::tests::child_lock_holder", "--exact"])
        .env("KEEL_GRAPH_LOCK_CHILD", dir.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dir.path().join("ready").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = dir.path().join("ready").exists();
    let blocked = matches!(try_acquire(dir.path()), Err(GraphLockError::Busy));
    child.kill().unwrap(); // SIGKILL on Unix, TerminateProcess on Windows; no destructors run.
    child.wait().unwrap();
    assert!(ready && blocked, "child must actually hold the lock");
    assert!(acquire(dir.path(), Duration::from_secs(1)).is_ok());
}
