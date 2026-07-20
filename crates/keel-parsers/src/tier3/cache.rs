//! SQLite-backed cache for Tier 3 resolution results.
//!
//! Cache entries are keyed by (file_path, line, callee_name, file_content_hash)
//! and invalidated when the content hash changes. Seeded from the persisted
//! `resolution_cache` table at the start of a `keel map` run and flushed back
//! at the end, so SCIP/LSP resolutions survive across runs.

use std::collections::HashMap;

use keel_core::types::ResolutionCacheEntry;

use crate::resolver::CallSite;

use super::provider::{Tier3CacheKey, Tier3Result};

/// Tier 3 resolution cache backed by SQLite.
pub struct Tier3Cache {
    /// This run's resolutions, keyed by the full call-site struct so
    /// `invalidate_file`/`invalidate_stale` can filter on `file_path`.
    entries: HashMap<Tier3CacheKey, CachedResolution>,
    /// Rows loaded from the persisted `resolution_cache` table, keyed by their
    /// content hash ([`Tier3CacheKey::cache_hash`]) since the original struct
    /// fields are not recoverable from a stored row. Read-only fallback for
    /// `get`; carried forward untouched on flush.
    persisted: HashMap<String, CachedResolution>,
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

impl CachedResolution {
    /// Reconstruct the public [`Tier3Result`] this cached row represents.
    fn to_result(&self) -> Tier3Result {
        if self.resolved {
            Tier3Result::Resolved {
                target_file: self.target_file.clone().unwrap_or_default(),
                target_name: self.target_name.clone().unwrap_or_default(),
                confidence: self.confidence,
                provider: self.provider.clone(),
            }
        } else {
            Tier3Result::Unresolved
        }
    }

    /// Convert into a persistable `resolution_cache` row under `call_site_hash`.
    fn to_entry(&self, call_site_hash: String) -> ResolutionCacheEntry {
        ResolutionCacheEntry {
            call_site_hash,
            target_file: self.target_file.clone(),
            target_name: self.target_name.clone(),
            confidence: self.confidence,
            provider: if self.provider.is_empty() {
                None
            } else {
                Some(self.provider.clone())
            },
        }
    }

    /// Build a cached row from a persisted `resolution_cache` entry (resolved
    /// iff `target_file` is present).
    fn from_entry(entry: &ResolutionCacheEntry) -> Self {
        Self {
            resolved: entry.target_file.is_some(),
            target_file: entry.target_file.clone(),
            target_name: entry.target_name.clone(),
            confidence: entry.confidence,
            provider: entry.provider.clone().unwrap_or_default(),
        }
    }
}

impl Tier3Cache {
    /// Creates a new empty Tier 3 resolution cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            persisted: HashMap::new(),
        }
    }

    /// Creates a cache pre-seeded with rows loaded from the `resolution_cache`
    /// table, so unchanged call sites hit without re-invoking a provider.
    pub fn with_seed(seed: Vec<ResolutionCacheEntry>) -> Self {
        let persisted = seed
            .iter()
            .map(|e| (e.call_site_hash.clone(), CachedResolution::from_entry(e)))
            .collect();
        Self {
            entries: HashMap::new(),
            persisted,
        }
    }

    /// Snapshot the cache as persistable `resolution_cache` rows.
    ///
    /// Unions the seeded `persisted` rows (carried forward untouched) with this
    /// run's freshly resolved `entries` (hashed via [`Tier3CacheKey::cache_hash`]).
    /// Fresh entries win on hash collision, so a re-resolved call site overwrites
    /// its stale persisted row.
    pub fn resolution_cache_entries(&self) -> Vec<ResolutionCacheEntry> {
        let mut by_hash: HashMap<String, ResolutionCacheEntry> =
            HashMap::with_capacity(self.persisted.len() + self.entries.len());
        for (hash, cached) in &self.persisted {
            by_hash.insert(hash.clone(), cached.to_entry(hash.clone()));
        }
        for (key, cached) in &self.entries {
            let hash = key.cache_hash();
            by_hash.insert(hash.clone(), cached.to_entry(hash));
        }
        by_hash.into_values().collect()
    }

    /// Look up a cached resolution for the given call site.
    ///
    /// Checks this run's `entries` first, then falls back to the seeded
    /// `persisted` rows via the key's content hash.
    pub fn get(&self, key: &Tier3CacheKey) -> Option<Tier3Result> {
        self.entries
            .get(key)
            .or_else(|| self.persisted.get(&key.cache_hash()))
            .map(CachedResolution::to_result)
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

    // --- persistence: seeding + flush (issue #44) ---

    fn resolved_entry(key: &Tier3CacheKey, target: &str) -> ResolutionCacheEntry {
        ResolutionCacheEntry {
            call_site_hash: key.cache_hash(),
            target_file: Some(target.into()),
            target_name: Some("foo".into()),
            confidence: 0.95,
            provider: Some("scip".into()),
        }
    }

    #[test]
    fn test_with_seed_get_hits_persisted() {
        let key = make_key("test.ts", 10, "foo", 12345);
        let cache = Tier3Cache::with_seed(vec![resolved_entry(&key, "other.ts")]);
        // `entries` is empty, so this hit comes from the seeded `persisted` map.
        assert!(cache.entries.is_empty());
        match cache.get(&key).expect("seeded row should hit") {
            Tier3Result::Resolved { target_file, .. } => assert_eq!(target_file, "other.ts"),
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn test_with_seed_get_misses_on_different_key() {
        let seeded = make_key("test.ts", 10, "foo", 12345);
        let cache = Tier3Cache::with_seed(vec![resolved_entry(&seeded, "other.ts")]);
        // A different content hash (file edited) must not hit the stale row.
        assert!(cache.get(&make_key("test.ts", 10, "foo", 99999)).is_none());
    }

    #[test]
    fn test_seeded_unresolved_roundtrips() {
        let key = make_key("test.ts", 10, "foo", 12345);
        let cache = Tier3Cache::with_seed(vec![ResolutionCacheEntry {
            call_site_hash: key.cache_hash(),
            target_file: None,
            target_name: None,
            confidence: 0.0,
            provider: None,
        }]);
        assert!(!cache
            .get(&key)
            .expect("seeded row should hit")
            .is_resolved());
    }

    #[test]
    fn test_resolution_cache_entries_passes_persisted_through() {
        let key = make_key("test.ts", 10, "foo", 12345);
        let cache = Tier3Cache::with_seed(vec![resolved_entry(&key, "other.ts")]);
        let out = cache.resolution_cache_entries();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].call_site_hash, key.cache_hash());
        assert_eq!(out[0].target_file.as_deref(), Some("other.ts"));
    }

    #[test]
    fn test_resolution_cache_entries_includes_new_entries() {
        let mut cache = Tier3Cache::new();
        let key = make_key("new.ts", 5, "bar", 42);
        cache.put(
            key.clone(),
            &Tier3Result::Resolved {
                target_file: "def.ts".into(),
                target_name: "bar".into(),
                confidence: 0.9,
                provider: "lsp".into(),
            },
        );
        let out = cache.resolution_cache_entries();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].call_site_hash, key.cache_hash());
        assert_eq!(out[0].provider.as_deref(), Some("lsp"));
    }

    #[test]
    fn test_resolution_cache_entries_new_wins_on_collision() {
        let key = make_key("test.ts", 10, "foo", 12345);
        // Seed a persisted row, then resolve the SAME call site this run to a
        // different target — the fresh entry must win.
        let mut cache = Tier3Cache::with_seed(vec![resolved_entry(&key, "STALE.ts")]);
        cache.put(
            key.clone(),
            &Tier3Result::Resolved {
                target_file: "FRESH.ts".into(),
                target_name: "foo".into(),
                confidence: 0.99,
                provider: "scip".into(),
            },
        );
        let out = cache.resolution_cache_entries();
        assert_eq!(out.len(), 1, "same hash must collapse to one row");
        assert_eq!(out[0].target_file.as_deref(), Some("FRESH.ts"));
    }

    /// The cross-run behavior this issue exists to deliver: a second run seeded
    /// with the first run's flush output hits the cache without re-resolving.
    #[test]
    fn test_cross_run_seed_produces_cache_hit() {
        // Run 1: cold cache, resolver is invoked and the result recorded.
        let mut run1 = Tier3Cache::new();
        let cs = make_call_site("test.ts", 10, "foo");
        let mut run1_calls = 0;
        run1.get_or_resolve(&cs, 777, |_| {
            run1_calls += 1;
            Tier3Result::Resolved {
                target_file: "def.ts".into(),
                target_name: "foo".into(),
                confidence: 0.95,
                provider: "scip".into(),
            }
        });
        assert_eq!(run1_calls, 1);

        // Persist run 1 and seed run 2 from it.
        let flushed = run1.resolution_cache_entries();
        let mut run2 = Tier3Cache::with_seed(flushed);

        // Run 2: same call site + unchanged content hash — resolver must NOT run.
        let mut run2_calls = 0;
        let result = run2.get_or_resolve(&cs, 777, |_| {
            run2_calls += 1;
            Tier3Result::Unavailable
        });
        assert_eq!(run2_calls, 0, "seeded row must satisfy the lookup");
        assert!(result.is_resolved());
    }
}
