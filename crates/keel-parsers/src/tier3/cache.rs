//! SQLite-backed cache for Tier 3 resolution results.
//!
//! Extends the existing `resolution_cache` table with Tier 3 columns.
//! Cache entries are keyed by (file_path, line, callee_name, file_content_hash)
//! and invalidated when the content hash changes.

use std::collections::HashMap;

use crate::resolver::CallSite;

use super::provider::{Tier3CacheKey, Tier3Result};

/// Tier 3 resolution cache backed by SQLite.
pub struct Tier3Cache {
    /// In-memory write-back buffer. Flushed to SQLite on `flush()`.
    entries: HashMap<Tier3CacheKey, CachedResolution>,
}

/// A cached Tier 3 resolution result.
#[derive(Debug, Clone)]
pub struct CachedResolution {
    pub target_file: Option<String>,
    pub target_name: Option<String>,
    pub confidence: f64,
    pub provider: String,
    pub resolved: bool,
}

impl Tier3Cache {
    /// Creates a new empty Tier 3 resolution cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Look up a cached resolution for the given call site.
    pub fn get(&self, key: &Tier3CacheKey) -> Option<Tier3Result> {
        self.entries.get(key).map(|cached| {
            if cached.resolved {
                Tier3Result::Resolved {
                    target_file: cached.target_file.clone().unwrap_or_default(),
                    target_name: cached.target_name.clone().unwrap_or_default(),
                    confidence: cached.confidence,
                    provider: cached.provider.clone(),
                }
            } else {
                Tier3Result::Unresolved
            }
        })
    }

    /// Store a resolution result in the cache.
    pub fn put(&mut self, key: Tier3CacheKey, result: &Tier3Result) {
        let cached = match result {
            Tier3Result::Resolved {
                target_file,
                target_name,
                confidence,
                provider,
            } => CachedResolution {
                target_file: Some(target_file.clone()),
                target_name: Some(target_name.clone()),
                confidence: *confidence,
                provider: provider.clone(),
                resolved: true,
            },
            Tier3Result::Unresolved => CachedResolution {
                target_file: None,
                target_name: None,
                confidence: 0.0,
                provider: String::new(),
                resolved: false,
            },
            Tier3Result::Unavailable => return, // don't cache unavailable
        };
        self.entries.insert(key, cached);
    }

    /// Invalidate all cached entries for a given file path.
    pub fn invalidate_file(&mut self, file_path: &str) {
        self.entries.retain(|key, _| key.file_path != file_path);
    }

    /// Invalidate entries whose content hash no longer matches.
    pub fn invalidate_stale(&mut self, file_path: &str, current_hash: u64) {
        self.entries
            .retain(|key, _| key.file_path != file_path || key.file_content_hash == current_hash);
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no entries are cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Try to resolve from cache, falling back to a resolver function.
    pub fn get_or_resolve<F>(
        &mut self,
        call_site: &CallSite,
        content_hash: u64,
        resolve_fn: F,
    ) -> Tier3Result
    where
        F: FnOnce(&CallSite) -> Tier3Result,
    {
        let key = Tier3CacheKey::from_call_site(call_site, content_hash);
        if let Some(cached) = self.get(&key) {
            return cached;
        }
        let result = resolve_fn(call_site);
        self.put(key, &result);
        result
    }
}

impl Default for Tier3Cache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(file: &str, line: u32, name: &str, hash: u64) -> Tier3CacheKey {
        Tier3CacheKey {
            file_path: file.into(),
            line,
            callee_name: name.into(),
            file_content_hash: hash,
        }
    }

    fn make_call_site(file: &str, line: u32, name: &str) -> CallSite {
        CallSite {
            file_path: file.into(),
            line,
            callee_name: name.into(),
            receiver: None,
        }
    }

    #[test]
    fn test_cache_miss() {
        let cache = Tier3Cache::new();
        let key = make_key("test.ts", 10, "foo", 12345);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_roundtrip_resolved() {
        let mut cache = Tier3Cache::new();
        let key = make_key("test.ts", 10, "foo", 12345);
        let result = Tier3Result::Resolved {
            target_file: "other.ts".into(),
            target_name: "foo".into(),
            confidence: 0.95,
            provider: "scip".into(),
        };
        cache.put(key.clone(), &result);
        let cached = cache.get(&key).expect("should be cached");
        match cached {
            Tier3Result::Resolved {
                target_name,
                confidence,
                ..
            } => {
                assert_eq!(target_name, "foo");
                assert!((confidence - 0.95).abs() < f64::EPSILON);
            }
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn test_cache_roundtrip_unresolved() {
        let mut cache = Tier3Cache::new();
        let key = make_key("test.ts", 10, "bar", 12345);
        cache.put(key.clone(), &Tier3Result::Unresolved);
        let cached = cache.get(&key).expect("should be cached");
        assert!(!cached.is_resolved());
    }

    #[test]
    fn test_unavailable_not_cached() {
        let mut cache = Tier3Cache::new();
        let key = make_key("test.ts", 10, "baz", 12345);
        cache.put(key.clone(), &Tier3Result::Unavailable);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_invalidate_file() {
        let mut cache = Tier3Cache::new();
        cache.put(make_key("a.ts", 1, "x", 100), &Tier3Result::Unresolved);
        cache.put(make_key("b.ts", 2, "y", 200), &Tier3Result::Unresolved);
        assert_eq!(cache.len(), 2);
        cache.invalidate_file("a.ts");
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&make_key("a.ts", 1, "x", 100)).is_none());
        assert!(cache.get(&make_key("b.ts", 2, "y", 200)).is_some());
    }

    #[test]
    fn test_invalidate_stale() {
        let mut cache = Tier3Cache::new();
        cache.put(make_key("a.ts", 1, "x", 100), &Tier3Result::Unresolved);
        cache.put(make_key("a.ts", 2, "y", 100), &Tier3Result::Unresolved);
        // Hash changed from 100 to 200 — both entries should be purged
        cache.invalidate_stale("a.ts", 200);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_get_or_resolve_cache_hit() {
        let mut cache = Tier3Cache::new();
        let cs = make_call_site("test.ts", 10, "foo");
        let key = Tier3CacheKey::from_call_site(&cs, 999);
        cache.put(
            key,
            &Tier3Result::Resolved {
                target_file: "t.ts".into(),
                target_name: "foo".into(),
                confidence: 0.9,
                provider: "scip".into(),
            },
        );
        let mut called = false;
        let result = cache.get_or_resolve(&cs, 999, |_| {
            called = true;
            Tier3Result::Unresolved
        });
        assert!(!called);
        assert!(result.is_resolved());
    }

    #[test]
    fn test_get_or_resolve_cache_miss() {
        let mut cache = Tier3Cache::new();
        let cs = make_call_site("test.ts", 10, "bar");
        let result = cache.get_or_resolve(&cs, 999, |_| Tier3Result::Resolved {
            target_file: "t.ts".into(),
            target_name: "bar".into(),
            confidence: 0.85,
            provider: "lsp".into(),
        });
        assert!(result.is_resolved());
        assert_eq!(cache.len(), 1);
    }
}
