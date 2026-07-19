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
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum time to wait for a matching response before degrading to an error.
///
/// A stalled or wedged language server must not block keel indefinitely — the
/// caller (`LspProvider::resolve`) turns any `send_request` error into
/// `Unresolved`, so a timeout simply drops the site to a lower resolution tier.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

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
///
/// A dedicated reader thread owns the child's stdout and forwards every decoded
/// frame over a channel, so `send_request` can wait on `recv_timeout` and give
/// up on a wedged server instead of blocking forever on a raw pipe read.
pub struct LspTransport {
    stdin: Mutex<BufWriter<ChildStdin>>,
    /// Frames from the reader thread. `Ok` is a decoded message body; `Err` is
    /// a terminal read/framing failure (EOF, malformed header), after which the
    /// reader thread has exited.
    responses: Mutex<Receiver<Result<Vec<u8>, String>>>,
    id_counter: AtomicU64,
    timeout: Duration,
}

impl LspTransport {
    /// Creates a new transport wrapping a child process's stdin/stdout pair,
    /// using the [`DEFAULT_TIMEOUT`] response deadline.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self::with_timeout(stdin, stdout, DEFAULT_TIMEOUT)
    }

    /// Creates a transport with an explicit response deadline (used by tests to
    /// keep the never-responds case fast).
    pub fn with_timeout(stdin: ChildStdin, stdout: ChildStdout, timeout: Duration) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::io::BufReader::new(stdout);
        // Detached: the thread exits on its own when stdout hits EOF (child
        // died or was killed), so there is nothing to join at drop time.
        std::thread::spawn(move || reader_loop(reader, tx));
        Self {
            stdin: Mutex::new(BufWriter::new(stdin)),
            responses: Mutex::new(rx),
            id_counter: AtomicU64::new(1),
            timeout,
        }
    }

    /// Sends a request and waits for the response whose id matches.
    ///
    /// Encodes the request as `Content-Length: N\r\n\r\n{json}`, writes it to
    /// the child's stdin, then reads decoded frames from the reader thread,
    /// skipping notifications and any response for a different id until the
    /// matching one arrives. Returns an error string on I/O, parse, or timeout.
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

        // Hold the response channel for the whole exchange so two concurrent
        // requests can't consume each other's frames.
        let responses = self
            .responses
            .lock()
            .map_err(|_| "response channel poisoned")?;

        {
            let mut stdin = self.stdin.lock().map_err(|_| "stdin lock poisoned")?;
            stdin
                .write_all(&frame)
                .map_err(|e| format!("write error: {e}"))?;
            stdin.flush().map_err(|e| format!("flush error: {e}"))?;
        }

        recv_matching(&responses, id, self.timeout)
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
// Reader thread + response matching
// ---------------------------------------------------------------------------

/// Continuously decode frames from `reader` and forward them over `tx`.
///
/// Runs on a dedicated thread. Exits when the receiver is dropped (transport
/// gone) or on the first terminal read error (EOF, malformed framing), which it
/// forwards so a blocked `recv_matching` unblocks with a real error.
pub(crate) fn reader_loop(mut reader: impl BufRead, tx: Sender<Result<Vec<u8>, String>>) {
    loop {
        match decode_message(&mut reader) {
            Ok(frame) => {
                if tx.send(Ok(frame)).is_err() {
                    break; // receiver dropped: transport was dropped
                }
            }
            Err(e) => {
                let _ = tx.send(Err(e));
                break; // unrecoverable stream state
            }
        }
    }
}

/// Wait for the response whose `id` matches `id`, skipping notifications
/// (no id) and responses for other ids, bounded by `timeout`.
///
/// A frame that fails to parse as a JSON-RPC response is skipped too (it may be
/// a server-to-client request); the per-recv timeout still bounds the wait.
pub(crate) fn recv_matching(
    responses: &Receiver<Result<Vec<u8>, String>>,
    id: u64,
    timeout: Duration,
) -> Result<JsonRpcResponse, String> {
    loop {
        match responses.recv_timeout(timeout) {
            Ok(Ok(frame)) => match serde_json::from_slice::<JsonRpcResponse>(&frame) {
                Ok(resp) if resp.id == Some(id) => return Ok(resp),
                // Notification (id absent) or a different id: keep waiting.
                _ => continue,
            },
            Ok(Err(e)) => return Err(e),
            Err(RecvTimeoutError::Timeout) => {
                return Err(format!("timeout waiting for response to request {id}"))
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("lsp reader thread ended before responding".into())
            }
        }
    }
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

    /// Frame a JSON body the way an LSP server would.
    fn frame(json: &str) -> Vec<u8> {
        encode_message(json.as_bytes())
    }

    #[test]
    fn test_reader_loop_forwards_frames_then_eof() {
        // Two framed messages back to back, then EOF.
        let mut stream = Vec::new();
        stream.extend_from_slice(&frame(r#"{"jsonrpc":"2.0","method":"log"}"#));
        stream.extend_from_slice(&frame(r#"{"jsonrpc":"2.0","id":1,"result":null}"#));
        let (tx, rx) = std::sync::mpsc::channel();
        reader_loop(Cursor::new(stream), tx);

        // Two Ok frames, then a terminal Err from the EOF.
        assert!(matches!(rx.recv(), Ok(Ok(_))));
        assert!(matches!(rx.recv(), Ok(Ok(_))));
        assert!(matches!(rx.recv(), Ok(Err(_))));
    }

    #[test]
    fn test_recv_matching_skips_notification_before_response() {
        // A server emits a notification (no id) before the real response.
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(
            br#"{"jsonrpc":"2.0","method":"window/logMessage","params":{}}"#.to_vec(),
        ))
        .unwrap();
        tx.send(Ok(
            br#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#.to_vec()
        ))
        .unwrap();

        let resp = recv_matching(&rx, 7, Duration::from_secs(1)).unwrap();
        assert_eq!(resp.id, Some(7));
        assert_eq!(resp.result, Some(serde_json::json!({"ok": true})));
    }

    #[test]
    fn test_recv_matching_skips_non_matching_id() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(br#"{"jsonrpc":"2.0","id":1,"result":"stale"}"#.to_vec()))
            .unwrap();
        tx.send(Ok(br#"{"jsonrpc":"2.0","id":2,"result":"mine"}"#.to_vec()))
            .unwrap();

        let resp = recv_matching(&rx, 2, Duration::from_secs(1)).unwrap();
        assert_eq!(resp.id, Some(2));
        assert_eq!(resp.result, Some(serde_json::json!("mine")));
    }

    #[test]
    fn test_recv_matching_times_out_when_no_response() {
        // Keep the sender alive (not disconnected) but never send: must time
        // out rather than block forever.
        let (_tx, rx) = std::sync::mpsc::channel::<Result<Vec<u8>, String>>();
        let err = recv_matching(&rx, 1, Duration::from_millis(50)).unwrap_err();
        assert!(err.contains("timeout"), "got: {err}");
    }

    #[test]
    fn test_recv_matching_reports_reader_error() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Err("server closed connection".to_string()))
            .unwrap();
        let err = recv_matching(&rx, 1, Duration::from_secs(1)).unwrap_err();
        assert!(err.contains("server closed"), "got: {err}");
    }
}
