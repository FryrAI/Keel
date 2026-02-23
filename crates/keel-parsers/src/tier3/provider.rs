//! Tier 3 provider trait and result types.
//!
//! Tier 3 resolution is a **separate concern** from the `LanguageResolver` trait
//! (which is a frozen contract). When `resolve_call_edge()` returns `None`,
//! the mapping pipeline falls back to Tier 3 providers registered here.

use std::path::Path;

use crate::resolver::CallSite;

/// Result of a Tier 3 resolution attempt.
#[derive(Debug, Clone)]
pub enum Tier3Result {
    /// Successfully resolved to a concrete definition.
    Resolved {
        target_file: String,
        target_name: String,
        confidence: f64,
        provider: String,
    },
    /// Provider tried but couldn't resolve (ambiguous, missing info).
    Unresolved,
    /// Provider doesn't handle this language or isn't available.
    Unavailable,
}

impl Tier3Result {
    /// Returns true if this result represents a successfully resolved definition.
    pub fn is_resolved(&self) -> bool {
        matches!(self, Tier3Result::Resolved { .. })
    }
}

/// A Tier 3 resolution provider (SCIP index or LSP server).
///
/// Implementors provide high-accuracy resolution for call sites that
/// Tier 1 (tree-sitter) and Tier 2 (per-language enhancers) couldn't resolve.
pub trait Tier3Provider: Send + Sync {
    /// Returns the canonical language name this provider handles.
    fn language(&self) -> &str;

    /// Check whether the provider is ready (index loaded, server running).
    fn is_available(&self) -> bool;

    /// Attempt to resolve a single call site.
    fn resolve(&self, call_site: &CallSite) -> Tier3Result;

    /// Resolve a batch of call sites. Default implementation calls `resolve`
    /// in a loop, but providers can override for efficiency.
    fn resolve_batch(&self, call_sites: &[CallSite]) -> Vec<Tier3Result> {
        call_sites.iter().map(|cs| self.resolve(cs)).collect()
    }

    /// Invalidate cached data for a file (content changed).
    fn invalidate_file(&self, file_path: &Path);

    /// Shut down the provider gracefully (stop servers, release memory).
    fn shutdown(&self) {}
}

/// Cache key for Tier 3 resolution results.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Tier3CacheKey {
    pub file_path: String,
    pub line: u32,
    pub callee_name: String,
    pub file_content_hash: u64,
}

impl Tier3CacheKey {
    /// Creates a cache key from a call site and the file's content hash.
    pub fn from_call_site(call_site: &CallSite, content_hash: u64) -> Self {
        Self {
            file_path: call_site.file_path.clone(),
            line: call_site.line,
            callee_name: call_site.callee_name.clone(),
            file_content_hash: content_hash,
        }
    }
}
