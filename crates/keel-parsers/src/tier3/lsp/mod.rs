//! LSP-based Tier 3 resolution provider.
//!
//! Spawns language servers lazily on first use and resolves call sites via
//! `textDocument/definition`. Only the minimal subset of the LSP protocol
//! required for definition lookup is implemented.
//!
//! This module is feature-gated: `#[cfg(feature = "tier3")]`.

pub mod protocol;
pub mod transport;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde_json::json;

use crate::resolver::CallSite;
use crate::tier3::provider::{Tier3Provider, Tier3Result};

use protocol::{
    file_path_to_uri, uri_to_file_path, ClientCapabilities, InitializeParams, InitializeResult,
    Location, Position, TextDocumentIdentifier, TextDocumentPositionParams,
};

// ---------------------------------------------------------------------------
// Default LSP server commands per language
// ---------------------------------------------------------------------------

fn default_lsp_command(language: &str) -> Option<(&'static str, &'static [&'static str])> {
    match language {
        "typescript" | "javascript" => Some(("typescript-language-server", &["--stdio"])),
        "python" => Some(("pyright-langserver", &["--stdio"])),
        "go" => Some(("gopls", &["serve"])),
        "rust" => Some(("rust-analyzer", &[])),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// LspProvider
// ---------------------------------------------------------------------------

/// Tier 3 provider that resolves call sites using a language server over stdio.
///
/// The server is spawned lazily on the first `resolve()` call and kept alive
/// for the lifetime of this struct. Any failure during initialization or
/// resolution returns `Unresolved` rather than propagating an error, preserving
/// the graceful-degradation contract of Tier 3.
pub struct LspProvider {
    language: String,
    command: String,
    args: Vec<String>,
    root_path: PathBuf,
    transport: Mutex<Option<transport::LspTransport>>,
    process: Mutex<Option<Child>>,
    initialized: AtomicBool,
}

impl LspProvider {
    /// Creates an `LspProvider` with an explicit server command and arguments.
    pub fn new(language: &str, command: &str, args: &[String], root_path: PathBuf) -> Self {
        Self {
            language: language.to_owned(),
            command: command.to_owned(),
            args: args.to_vec(),
            root_path,
            transport: Mutex::new(None),
            process: Mutex::new(None),
            initialized: AtomicBool::new(false),
        }
    }

    /// Creates an `LspProvider` using the built-in default command for `language`.
    ///
    /// Returns `None` if no default is registered for the given language.
    pub fn from_defaults(language: &str, root_path: PathBuf) -> Option<Self> {
        let (cmd, static_args) = default_lsp_command(language)?;
        let args: Vec<String> = static_args.iter().map(|s| s.to_string()).collect();
        Some(Self::new(language, cmd, &args, root_path))
    }

    /// Ensures the language server is running and the `initialize` handshake
    /// has completed. Returns `true` on success, `false` on any failure.
    ///
    /// Idempotent: subsequent calls after a successful initialization return
    /// `true` immediately.
    fn ensure_initialized(&self) -> bool {
        if self.initialized.load(Ordering::Acquire) {
            return true;
        }

        let (child, xport) = match self.spawn_server() {
            Ok(pair) => pair,
            Err(_) => return false,
        };

        let root_uri = file_path_to_uri(self.root_path.to_string_lossy().as_ref());
        let params = InitializeParams {
            root_uri,
            capabilities: ClientCapabilities {},
        };
        let params_value = match serde_json::to_value(&params) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let response = match xport.send_request("initialize", params_value) {
            Ok(r) => r,
            Err(_) => return false,
        };

        // Validate that the server responded without error.
        if response.error.is_some() {
            return false;
        }
        // Parse capabilities (we don't use them, but verify the response shape).
        if let Some(result_val) = response.result {
            if serde_json::from_value::<InitializeResult>(result_val).is_err() {
                return false;
            }
        }

        // Send initialized notification to complete the handshake.
        let _ = xport.send_notification("initialized", json!({}));

        {
            let mut t = match self.transport.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            *t = Some(xport);
        }
        {
            let mut p = match self.process.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            *p = Some(child);
        }

        self.initialized.store(true, Ordering::Release);
        true
    }

    /// Spawns the language server subprocess with stdin/stdout piped.
    fn spawn_server(&self) -> Result<(Child, transport::LspTransport), String> {
        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn '{}': {e}", self.command))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "child stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "child stdout unavailable".to_string())?;

        Ok((child, transport::LspTransport::new(stdin, stdout)))
    }
}

impl Tier3Provider for LspProvider {
    fn language(&self) -> &str {
        &self.language
    }

    /// Always returns `true`; availability is determined lazily during
    /// `resolve()`. This avoids expensive PATH probing on the hot path.
    fn is_available(&self) -> bool {
        true
    }

    /// Resolves a call site using `textDocument/definition`.
    ///
    /// Returns `Unresolved` on any LSP or I/O error rather than propagating
    /// the failure, honoring the Tier 3 graceful-degradation contract.
    fn resolve(&self, call_site: &CallSite) -> Tier3Result {
        if !self.ensure_initialized() {
            return Tier3Result::Unresolved;
        }

        let transport_guard = match self.transport.lock() {
            Ok(g) => g,
            Err(_) => return Tier3Result::Unresolved,
        };
        let xport = match transport_guard.as_ref() {
            Some(t) => t,
            None => return Tier3Result::Unresolved,
        };

        let uri = file_path_to_uri(&call_site.file_path);
        // LSP positions are 0-based; CallSite.line is 1-based.
        let line = call_site.line.saturating_sub(1);
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position { line, character: 0 },
        };
        let params_value = match serde_json::to_value(&params) {
            Ok(v) => v,
            Err(_) => return Tier3Result::Unresolved,
        };

        let response = match xport.send_request("textDocument/definition", params_value) {
            Ok(r) => r,
            Err(_) => return Tier3Result::Unresolved,
        };

        if response.error.is_some() {
            return Tier3Result::Unresolved;
        }

        let result_val = match response.result {
            Some(v) => v,
            None => return Tier3Result::Unresolved,
        };

        // The spec allows a single Location, an array of Locations, or null.
        let location: Location = if result_val.is_array() {
            match serde_json::from_value::<Vec<Location>>(result_val) {
                Ok(locs) if !locs.is_empty() => locs.into_iter().next().unwrap(),
                _ => return Tier3Result::Unresolved,
            }
        } else if result_val.is_null() {
            return Tier3Result::Unresolved;
        } else {
            match serde_json::from_value::<Location>(result_val) {
                Ok(loc) => loc,
                Err(_) => return Tier3Result::Unresolved,
            }
        };

        let target_file = match uri_to_file_path(&location.uri) {
            Some(p) => p,
            None => return Tier3Result::Unresolved,
        };

        Tier3Result::Resolved {
            target_file,
            target_name: call_site.callee_name.clone(),
            confidence: 0.90,
            provider: "lsp".into(),
        }
    }

    /// No-op: LSP servers track open files internally via `textDocument/didOpen`
    /// notifications, which this minimal client does not send.
    fn invalidate_file(&self, _file_path: &Path) {}

    /// Sends `shutdown` + `exit` to the language server and kills the process.
    fn shutdown(&self) {
        if !self.initialized.load(Ordering::Acquire) {
            return;
        }

        if let Ok(guard) = self.transport.lock() {
            if let Some(xport) = guard.as_ref() {
                let _ = xport.send_request("shutdown", json!(null));
                let _ = xport.send_notification("exit", json!(null));
            }
        }

        if let Ok(mut guard) = self.process.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
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

    fn dummy_root() -> PathBuf {
        PathBuf::from("/tmp")
    }

    #[test]
    fn test_bad_command_resolve_returns_unresolved() {
        let provider = LspProvider::new(
            "typescript",
            "nonexistent-lsp-server-xyzzy",
            &[],
            dummy_root(),
        );
        let cs = CallSite {
            file_path: "/tmp/test.ts".into(),
            line: 10,
            callee_name: "foo".into(),
            receiver: None,
        };
        // Spawning fails -> ensure_initialized returns false -> Unresolved.
        let result = provider.resolve(&cs);
        assert!(!result.is_resolved());
    }

    #[test]
    fn test_from_defaults_known_languages() {
        for lang in &["typescript", "javascript", "python", "go", "rust"] {
            let provider = LspProvider::from_defaults(lang, dummy_root());
            assert!(provider.is_some(), "expected Some for language '{lang}'");
            let p = provider.unwrap();
            assert_eq!(p.language(), *lang);
        }
    }

    #[test]
    fn test_from_defaults_unknown_language_returns_none() {
        assert!(LspProvider::from_defaults("cobol", dummy_root()).is_none());
    }

    #[test]
    fn test_is_available_always_true() {
        let p = LspProvider::new("python", "pyright-langserver", &[], dummy_root());
        assert!(p.is_available());
    }

    #[test]
    fn test_language_accessor() {
        let p = LspProvider::new("go", "gopls", &["serve".into()], dummy_root());
        assert_eq!(p.language(), "go");
    }
}
