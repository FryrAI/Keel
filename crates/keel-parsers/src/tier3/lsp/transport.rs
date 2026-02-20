//! JSON-RPC 2.0 over stdio transport for LSP communication.
//!
//! The LSP wire format wraps each message with an HTTP-style header:
//!
//! ```text
//! Content-Length: 123\r\n
//! \r\n
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
//! ```
//!
//! All I/O uses `std::sync::Mutex` so that `LspTransport` is `Send + Sync`.

use std::io::{BufRead, BufWriter, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

/// An outbound JSON-RPC request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

/// An inbound JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object embedded in a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// Bidirectional JSON-RPC transport backed by a child process's stdio pipes.
pub struct LspTransport {
    stdin: Mutex<BufWriter<ChildStdin>>,
    stdout: Mutex<std::io::BufReader<ChildStdout>>,
    id_counter: AtomicU64,
}

impl LspTransport {
    /// Creates a new transport wrapping a child process's stdin/stdout pair.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin: Mutex::new(BufWriter::new(stdin)),
            stdout: Mutex::new(std::io::BufReader::new(stdout)),
            id_counter: AtomicU64::new(1),
        }
    }

    /// Sends a request and waits for the matching response.
    ///
    /// Encodes the request as `Content-Length: N\r\n\r\n{json}`, writes it to
    /// the child's stdin, then reads and decodes the next message from stdout.
    ///
    /// Returns an error string on any I/O or parse failure.
    pub fn send_request(&self, method: &str, params: Value) -> Result<JsonRpcResponse, String> {
        let id = self.next_id();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };
        let body = serde_json::to_vec(&request).map_err(|e| format!("serialize error: {e}"))?;
        let frame = encode_message(&body);

        {
            let mut stdin = self.stdin.lock().map_err(|_| "stdin lock poisoned")?;
            stdin
                .write_all(&frame)
                .map_err(|e| format!("write error: {e}"))?;
            stdin.flush().map_err(|e| format!("flush error: {e}"))?;
        }

        let raw = {
            let mut stdout = self.stdout.lock().map_err(|_| "stdout lock poisoned")?;
            decode_message(&mut *stdout)?
        };

        serde_json::from_slice::<JsonRpcResponse>(&raw)
            .map_err(|e| format!("deserialize response error: {e}"))
    }

    /// Sends a JSON-RPC notification (no id, no response expected).
    pub fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        // Notifications omit the `id` field entirely per the JSON-RPC spec.
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let body =
            serde_json::to_vec(&notification).map_err(|e| format!("serialize error: {e}"))?;
        let frame = encode_message(&body);

        let mut stdin = self.stdin.lock().map_err(|_| "stdin lock poisoned")?;
        stdin
            .write_all(&frame)
            .map_err(|e| format!("write error: {e}"))?;
        stdin.flush().map_err(|e| format!("flush error: {e}"))?;
        Ok(())
    }

    /// Returns the next monotonically increasing request id.
    fn next_id(&self) -> u64 {
        self.id_counter.fetch_add(1, Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Framing helpers (pub(crate) for tests in mod.rs)
// ---------------------------------------------------------------------------

/// Prepends `Content-Length: {len}\r\n\r\n` to `msg`.
pub(crate) fn encode_message(msg: &[u8]) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", msg.len());
    let mut out = Vec::with_capacity(header.len() + msg.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(msg);
    out
}

/// Reads one LSP message from `reader`: parses `Content-Length` header then
/// reads exactly that many bytes of payload.
pub(crate) fn decode_message(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut content_length: Option<usize> = None;

    // Read headers until the blank separator line.
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("header read error: {e}"))?;
        if n == 0 {
            return Err("server closed connection".into());
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // blank line separates headers from body
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|e| format!("bad Content-Length: {e}"))?,
            );
        }
        // Other headers (Content-Type etc.) are silently ignored.
    }

    let len = content_length.ok_or("no Content-Length header found")?;
    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("body read error: {e}"))?;
    Ok(body)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_encode_message_format() {
        let payload = b"{\"jsonrpc\":\"2.0\"}";
        let framed = encode_message(payload);
        let expected_header = format!("Content-Length: {}\r\n\r\n", payload.len());
        assert!(framed.starts_with(expected_header.as_bytes()));
        assert!(framed.ends_with(payload));
        assert_eq!(framed.len(), expected_header.len() + payload.len());
    }

    #[test]
    fn test_decode_message_reads_body() {
        let payload = br#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let mut raw = Vec::new();
        raw.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
        raw.extend_from_slice(payload);
        let mut cursor = Cursor::new(raw);
        let body = decode_message(&mut cursor).unwrap();
        assert_eq!(body, payload);
    }

    #[test]
    fn test_decode_message_ignores_extra_headers() {
        let payload = b"hello";
        let mut raw = Vec::new();
        raw.extend_from_slice(b"Content-Type: application/json\r\n");
        raw.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
        raw.extend_from_slice(payload);
        let mut cursor = Cursor::new(raw);
        let body = decode_message(&mut cursor).unwrap();
        assert_eq!(body, payload);
    }

    #[test]
    fn test_decode_message_error_on_missing_content_length() {
        let mut cursor = Cursor::new(b"X-Custom: foo\r\n\r\n".to_vec());
        let result = decode_message(&mut cursor);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Content-Length"));
    }

    #[test]
    fn test_id_counter_increments() {
        let counter = AtomicU64::new(1);
        let id1 = counter.fetch_add(1, Ordering::Relaxed);
        let id2 = counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }
}
