//! Tier 3 resolution: LSP/SCIP on-demand providers.
//!
//! This module implements the third tier of keel's hybrid resolution strategy.
//! When Tier 1 (tree-sitter) and Tier 2 (per-language enhancers) can't resolve
//! a call edge, Tier 3 providers attempt resolution using:
//!
//! - **SCIP** (preferred): Pre-built protobuf indexes from Sourcegraph tooling.
//! - **LSP** (fallback): Language servers spawned lazily over stdio JSON-RPC.
//!
//! Tier 3 is feature-gated behind `tier3` (on by default). When disabled,
//! this module compiles to empty stubs.

pub mod cache;
pub mod provider;

#[cfg(feature = "tier3")]
pub mod scip;

#[cfg(feature = "tier3")]
pub mod lsp;

use crate::resolver::CallSite;
use provider::{Tier3Provider, Tier3Result};

/// Registry of Tier 3 providers, tried in order for each unresolved call site.
pub struct Tier3Registry {
    providers: Vec<Box<dyn Tier3Provider>>,
}

impl Tier3Registry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a provider. Providers are tried in registration order.
    pub fn register(&mut self, provider: Box<dyn Tier3Provider>) {
        self.providers.push(provider);
    }

    /// Attempt to resolve a call site through all registered providers.
    /// Returns the first `Resolved` result, or `Unresolved` if none succeed.
    pub fn resolve(&self, call_site: &CallSite) -> Tier3Result {
        for provider in &self.providers {
            if !provider.is_available() {
                continue;
            }
            let result = provider.resolve(call_site);
            if result.is_resolved() {
                return result;
            }
        }
        Tier3Result::Unresolved
    }

    /// Resolve a batch of call sites. For each site, tries providers in order.
    pub fn resolve_batch(&self, call_sites: &[CallSite]) -> Vec<Tier3Result> {
        call_sites.iter().map(|cs| self.resolve(cs)).collect()
    }

    /// Invalidate cached data for a file across all providers.
    pub fn invalidate_file(&self, file_path: &std::path::Path) {
        for provider in &self.providers {
            provider.invalidate_file(file_path);
        }
    }

    /// Shut down all providers gracefully.
    pub fn shutdown(&self) {
        for provider in &self.providers {
            provider.shutdown();
        }
    }

    /// Returns the number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Returns true if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for Tier3Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct MockProvider {
        lang: String,
        available: bool,
        result: Tier3Result,
    }

    impl Tier3Provider for MockProvider {
        fn language(&self) -> &str {
            &self.lang
        }
        fn is_available(&self) -> bool {
            self.available
        }
        fn resolve(&self, _call_site: &CallSite) -> Tier3Result {
            self.result.clone()
        }
        fn invalidate_file(&self, _file_path: &Path) {}
    }

    #[test]
    fn test_empty_registry_returns_unresolved() {
        let registry = Tier3Registry::new();
        let cs = CallSite {
            file_path: "test.ts".into(),
            line: 10,
            callee_name: "foo".into(),
            receiver: None,
        };
        assert!(!registry.resolve(&cs).is_resolved());
    }

    #[test]
    fn test_registry_skips_unavailable_providers() {
        let mut registry = Tier3Registry::new();
        registry.register(Box::new(MockProvider {
            lang: "typescript".into(),
            available: false,
            result: Tier3Result::Resolved {
                target_file: "a.ts".into(),
                target_name: "foo".into(),
                confidence: 0.99,
                provider: "mock".into(),
            },
        }));
        let cs = CallSite {
            file_path: "test.ts".into(),
            line: 10,
            callee_name: "foo".into(),
            receiver: None,
        };
        assert!(!registry.resolve(&cs).is_resolved());
    }

    #[test]
    fn test_registry_returns_first_resolved() {
        let mut registry = Tier3Registry::new();
        registry.register(Box::new(MockProvider {
            lang: "typescript".into(),
            available: true,
            result: Tier3Result::Unresolved,
        }));
        registry.register(Box::new(MockProvider {
            lang: "typescript".into(),
            available: true,
            result: Tier3Result::Resolved {
                target_file: "b.ts".into(),
                target_name: "bar".into(),
                confidence: 0.95,
                provider: "mock2".into(),
            },
        }));
        let cs = CallSite {
            file_path: "test.ts".into(),
            line: 10,
            callee_name: "bar".into(),
            receiver: None,
        };
        match registry.resolve(&cs) {
            Tier3Result::Resolved { target_name, .. } => assert_eq!(target_name, "bar"),
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn test_provider_count() {
        let mut registry = Tier3Registry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.provider_count(), 0);
        registry.register(Box::new(MockProvider {
            lang: "python".into(),
            available: true,
            result: Tier3Result::Unresolved,
        }));
        assert_eq!(registry.provider_count(), 1);
        assert!(!registry.is_empty());
    }
}
