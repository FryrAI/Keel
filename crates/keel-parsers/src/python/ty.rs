use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Result from ty type-checking subprocess.
#[derive(Debug, Clone)]
pub struct TyResult {
    pub definitions: Vec<TyDefinition>,
    pub errors: Vec<TyError>,
}

/// A definition found by ty.
#[derive(Debug, Clone)]
pub struct TyDefinition {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
}

/// An error or diagnostic from ty.
#[derive(Debug, Clone)]
pub struct TyError {
    pub message: String,
    pub file_path: String,
    pub line: u32,
}

/// Trait for ty subprocess interaction (allows mocking in tests).
pub trait TyClient: Send + Sync {
    fn check_file(&self, path: &Path) -> Result<TyResult, TyError>;
    fn is_available(&self) -> bool;
}

/// Real ty subprocess client.
pub struct RealTyClient {
    timeout: Duration,
    cache: Mutex<HashMap<(PathBuf, u64), TyResult>>,
}

/// Content hash used to key the ty result cache.
///
/// Reads the file and hashes its bytes so a cached result is invalidated the
/// moment the source changes. Missing/unreadable files hash to 0 — ty will be
/// (re)invoked and simply fail, rather than returning a stale hit.
pub(crate) fn content_hash_for(path: &Path) -> u64 {
    match std::fs::read(path) {
        Ok(bytes) => xxhash_rust::xxh64::xxh64(&bytes, 0),
        Err(_) => 0,
    }
}

impl RealTyClient {
    /// Creates a new `RealTyClient` with a default 5-second timeout and empty cache.
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Detects if the `ty` binary is available on PATH and returns a client if found.
    pub fn detect() -> Option<Self> {
        match Command::new("ty").arg("--version").output() {
            Ok(output) if output.status.success() => Some(Self::new()),
            _ => None,
        }
    }

    /// Run `ty check` on a single file, bounded by `self.timeout`.
    ///
    /// ty is a subprocess (constitution: never a library), so a hung or slow
    /// process must never hang keel. The child is polled and hard-killed if it
    /// overruns the timeout; stdout is drained on a helper thread so a full
    /// pipe buffer cannot deadlock the poll loop.
    fn run_ty_check(&self, path: &Path) -> Result<String, TyError> {
        let err = |message: String| TyError {
            message,
            file_path: path.to_string_lossy().to_string(),
            line: 0,
        };

        let mut child = Command::new("ty")
            .args(["check", "--output-format", "json"])
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| err(format!("Failed to spawn ty: {e}")))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| err("ty produced no stdout handle".to_string()))?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = stdout.read_to_string(&mut buf);
            let _ = tx.send(buf);
        });
        // Drain stderr on its own thread (same as stdout) so a full pipe can
        // never deadlock the child; its content feeds the failure message.
        let stderr_rx = child.stderr.take().map(|mut se| {
            let (etx, erx) = mpsc::channel();
            thread::spawn(move || {
                let mut buf = String::new();
                let _ = se.read_to_string(&mut buf);
                let _ = etx.send(buf);
            });
            erx
        });

        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if start.elapsed() >= self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(err(format!(
                            "ty timed out after {}s",
                            self.timeout.as_secs()
                        )));
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(err(format!("ty wait failed: {e}"))),
            }
        };

        if !status.success() {
            let stderr_text = stderr_rx
                .and_then(|rx| rx.recv().ok())
                .unwrap_or_default();
            let detail = stderr_text.trim();
            return Err(err(if detail.is_empty() {
                format!("ty exited with status {status}")
            } else {
                format!("ty exited with status {status}: {detail}")
            }));
        }

        rx.recv()
            .map_err(|_| err("ty stdout reader dropped".to_string()))
    }
}

impl Default for RealTyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TyClient for RealTyClient {
    fn check_file(&self, path: &Path) -> Result<TyResult, TyError> {
        // Cache key is (path, content_hash): changing the file's content
        // invalidates any cached result instead of serving a stale hit.
        let cache_key = (path.to_path_buf(), content_hash_for(path));
        if let Some(cached) = self.cache.lock().unwrap().get(&cache_key) {
            return Ok(cached.clone());
        }

        let stdout = self.run_ty_check(path)?;
        let result = parse_ty_json_output(&stdout);

        self.cache.lock().unwrap().insert(cache_key, result.clone());

        Ok(result)
    }

    fn is_available(&self) -> bool {
        Command::new("ty")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Mock ty client for unit tests.
pub struct MockTyClient {
    available: bool,
    results: Mutex<HashMap<PathBuf, Result<TyResult, String>>>,
    /// Tracks how many times check_file was called per path.
    pub call_counts: Mutex<HashMap<PathBuf, usize>>,
}

impl MockTyClient {
    /// Creates a mock ty client with the given availability status.
    pub fn new(available: bool) -> Self {
        Self {
            available,
            results: Mutex::new(HashMap::new()),
            call_counts: Mutex::new(HashMap::new()),
        }
    }

    /// Sets a successful result to return when `check_file` is called for the given path.
    pub fn set_result(&self, path: PathBuf, result: TyResult) {
        self.results.lock().unwrap().insert(path, Ok(result));
    }

    /// Sets an error result to return when `check_file` is called for the given path.
    pub fn set_error(&self, path: PathBuf, error: String) {
        self.results.lock().unwrap().insert(path, Err(error));
    }

    /// Returns the number of times `check_file` was called for the given path.
    pub fn call_count(&self, path: &Path) -> usize {
        self.call_counts
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(0)
    }
}

impl TyClient for MockTyClient {
    fn check_file(&self, path: &Path) -> Result<TyResult, TyError> {
        // Track call count
        *self
            .call_counts
            .lock()
            .unwrap()
            .entry(path.to_path_buf())
            .or_insert(0) += 1;

        let results = self.results.lock().unwrap();
        match results.get(path) {
            Some(Ok(result)) => Ok(result.clone()),
            Some(Err(msg)) => Err(TyError {
                message: msg.clone(),
                file_path: path.to_string_lossy().to_string(),
                line: 0,
            }),
            None => Ok(TyResult {
                definitions: vec![],
                errors: vec![],
            }),
        }
    }

    fn is_available(&self) -> bool {
        self.available
    }
}

/// Parses ty subprocess JSON output into definitions and diagnostic errors.
pub fn parse_ty_json_output(json_str: &str) -> TyResult {
    // ty outputs JSON with diagnostic information
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => {
            return TyResult {
                definitions: vec![],
                errors: vec![],
            }
        }
    };

    let mut definitions = vec![];
    let mut errors = vec![];

    if let Some(diagnostics) = value.as_array() {
        for diag in diagnostics {
            if let Some(msg) = diag.get("message").and_then(|m| m.as_str()) {
                let file_path = diag
                    .get("file")
                    .and_then(|f| f.as_str())
                    .unwrap_or("")
                    .to_string();
                let line = diag.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;

                // Check if this is a definition or an error
                let severity = diag
                    .get("severity")
                    .and_then(|s| s.as_str())
                    .unwrap_or("error");

                if severity == "information" {
                    if let Some(name) = diag.get("name").and_then(|n| n.as_str()) {
                        let kind = diag
                            .get("kind")
                            .and_then(|k| k.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        definitions.push(TyDefinition {
                            name: name.to_string(),
                            kind,
                            file_path: file_path.clone(),
                            line,
                        });
                    }
                }

                errors.push(TyError {
                    message: msg.to_string(),
                    file_path,
                    line,
                });
            }
        }
    }

    TyResult {
        definitions,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_ty_client_available() {
        let client = MockTyClient::new(true);
        assert!(client.is_available());
    }

    #[test]
    fn test_mock_ty_client_unavailable() {
        let client = MockTyClient::new(false);
        assert!(!client.is_available());
    }

    #[test]
    fn test_parse_empty_json() {
        let result = parse_ty_json_output("[]");
        assert!(result.definitions.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_ty_json_output("not json");
        assert!(result.definitions.is_empty());
    }

    #[test]
    fn test_mock_tracks_call_count() {
        let client = MockTyClient::new(true);
        let path = Path::new("test.py");
        let _ = client.check_file(path);
        let _ = client.check_file(path);
        assert_eq!(client.call_count(path), 2);
    }

    #[test]
    fn test_content_hash_changes_with_content() {
        let dir = std::env::temp_dir().join(format!("keel_ty_hash_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.py");

        std::fs::write(&path, "def f(): pass\n").unwrap();
        let h1 = content_hash_for(&path);
        // Same content re-hashed is stable (cache hit).
        assert_eq!(h1, content_hash_for(&path));

        std::fs::write(&path, "def g(): pass\n").unwrap();
        let h2 = content_hash_for(&path);
        // Changed content invalidates the cache key.
        assert_ne!(h1, h2, "content hash must change when file content changes");

        std::fs::remove_file(&path).unwrap();
        // A missing file hashes to the 0 sentinel.
        assert_eq!(content_hash_for(&path), 0);
    }
}
