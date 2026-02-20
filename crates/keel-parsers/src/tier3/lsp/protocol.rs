//! Minimal LSP protocol types for initialize, textDocument/definition, and shutdown.
//!
//! Only the request/response types actually used by `LspProvider` are defined here.
//! Full LSP spec compliance is intentionally out of scope.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Initialization types
// ---------------------------------------------------------------------------

/// Parameters sent with the `initialize` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "rootUri")]
    pub root_uri: String,
    pub capabilities: ClientCapabilities,
}

/// Client capabilities sent during initialization.
///
/// Kept empty — we only need definition lookup, which all servers support
/// without any declared capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {}

/// Server response to `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
}

/// Server capabilities returned during initialization.
///
/// Kept empty — we only inspect whether the server started without error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {}

// ---------------------------------------------------------------------------
// Common document types
// ---------------------------------------------------------------------------

/// Identifies a text document by URI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

/// A zero-based line/character position within a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// A range within a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

// ---------------------------------------------------------------------------
// textDocument/definition types
// ---------------------------------------------------------------------------

/// Parameters for `textDocument/definition`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

/// A concrete location (file + range) returned by `textDocument/definition`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// Converts an absolute file path to a `file:///` URI.
///
/// On POSIX systems the path already starts with `/`, so the result is
/// `file:///absolute/path`.  Windows paths (`C:\…`) are not handled here
/// because keel targets Linux and macOS as primary platforms.
pub fn file_path_to_uri(path: &str) -> String {
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

/// Strips the `file:///` prefix from a URI and returns the path component.
///
/// Returns `None` if the URI does not start with `file://`.
pub fn uri_to_file_path(uri: &str) -> Option<String> {
    let without_scheme = uri.strip_prefix("file://")?;
    // `file:///abs/path` -> strip leading `/` only if followed by another char
    // that isn't a second slash (avoids mangling `file:////unc` paths).
    if without_scheme.starts_with('/') {
        Some(without_scheme.to_string())
    } else {
        // Relative URI or unusual form — return as-is.
        Some(without_scheme.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_roundtrip_absolute() {
        let path = "/home/user/project/src/main.rs";
        let uri = file_path_to_uri(path);
        assert_eq!(uri, "file:///home/user/project/src/main.rs");
        let recovered = uri_to_file_path(&uri).unwrap();
        assert_eq!(recovered, path);
    }

    #[test]
    fn test_uri_to_file_path_rejects_non_file_scheme() {
        assert!(uri_to_file_path("http://example.com/foo").is_none());
        assert!(uri_to_file_path("https://example.com/bar").is_none());
    }

    #[test]
    fn test_location_serializes() {
        let loc = Location {
            uri: "file:///foo/bar.ts".into(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
        };
        let json = serde_json::to_string(&loc).unwrap();
        let back: Location = serde_json::from_str(&json).unwrap();
        assert_eq!(back.uri, loc.uri);
        assert_eq!(back.range.start.line, 0);
    }

    #[test]
    fn test_initialize_params_serializes() {
        let params = InitializeParams {
            root_uri: "file:///project".into(),
            capabilities: ClientCapabilities {},
        };
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("rootUri"));
        assert!(json.contains("capabilities"));
    }
}
