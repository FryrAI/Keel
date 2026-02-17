use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

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
    #[allow(dead_code)]
    timeout: Duration,
    cache: Mutex<HashMap<(PathBuf, u64), TyResult>>,
}

impl RealTyClient {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Detect if ty binary is available on PATH.
    pub fn detect() -> Option<Self> {
        match Command::new("ty").arg("--version").output() {
            Ok(output) if output.status.success() => Some(Self::new()),
            _ => None,
        }
    }
}

impl Default for RealTyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TyClient for RealTyClient {
    fn check_file(&self, path: &Path) -> Result<TyResult, TyError> {
        let path_buf = path.to_path_buf();

        // Check cache first (use 0 as placeholder - production would use content hash)
        let cache_key = (path_buf, 0u64);
        if let Some(cached) = self.cache.lock().unwrap().get(&cache_key) {
            return Ok(cached.clone());
        }

        let output = Command::new("ty")
            .args(["check", "--output-format", "json"])
            .arg(path)
            .output()
            .map_err(|e| TyError {
                message: format!("Failed to spawn ty: {e}"),
                file_path: path.to_string_lossy().to_string(),
                line: 0,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TyError {
                message: format!("ty exited with status {}: {stderr}", output.status),
                file_path: path.to_string_lossy().to_string(),
                line: 0,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = parse_ty_json_output(&stdout);

        // Cache the result
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
    pub fn new(available: bool) -> Self {
        Self {
            available,
            results: Mutex::new(HashMap::new()),
            call_counts: Mutex::new(HashMap::new()),
        }
    }

    /// Set a successful result for a given path.
    pub fn set_result(&self, path: PathBuf, result: TyResult) {
        self.results
            .lock()
            .unwrap()
            .insert(path, Ok(result));
    }

    /// Set an error result for a given path.
    pub fn set_error(&self, path: PathBuf, error: String) {
        self.results
            .lock()
            .unwrap()
            .insert(path, Err(error));
    }

    /// Get the number of times check_file was called for a path.
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

/// Parse ty JSON output into TyResult.
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
                let line =
                    diag.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32;

                // Check if this is a definition or an error
                let severity = diag
                    .get("severity")
                    .and_then(|s| s.as_str())
                    .unwrap_or("error");

                if severity == "information" {
                    if let Some(name) =
                        diag.get("name").and_then(|n| n.as_str())
                    {
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
}
