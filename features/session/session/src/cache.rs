//! Thread-safe cache abstraction for session data.
//!
//! Provides `Cache<K, V>` backed by `Arc<RwLock<FxHashMap<K, V>>>`
//! with typed get/insert/invalidate operations. Designed to be
//! `Send + Sync` for use across async tasks.

use rustc_hash::FxHashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A thread-safe, async-compatible cache backed by `FxHashMap`.
///
/// All operations acquire the internal `RwLock` and are safe to call
/// concurrently from multiple tokio tasks.
#[derive(Debug)]
pub struct Cache<K, V> {
    inner: Arc<RwLock<FxHashMap<K, V>>>,
}

// Manual Clone so we don't require K: Clone, V: Clone on the struct itself.
impl<K, V> Clone for Cache<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Send + Sync,
    V: Send + Sync,
{
    /// Create an empty cache with no dashboard name.
    ///
    /// Prefer [`Cache::named`] when a descriptive name is available.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    /// Create an empty cache with a descriptive name (retained for API compatibility; the moire dashboard is retired).
    #[must_use]
    pub fn named(_name: &'static str) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    /// Return a clone of the value associated with `key`, if present.
    pub async fn get(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.inner.read().await.get(key).cloned()
    }

    /// Insert a key-value pair, replacing any previous value.
    pub async fn insert(&self, key: K, value: V) {
        self.inner.write().await.insert(key, value);
    }

    /// Remove the entry for `key`, if present.
    pub async fn invalidate(&self, key: &K) {
        self.inner.write().await.remove(key);
    }

    /// Remove all entries.
    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }

    /// Return the value for `key`, inserting it with `f()` if absent.
    pub async fn get_or_insert_with(&self, key: K, f: impl FnOnce() -> V + Send) -> V
    where
        V: Clone,
    {
        // Fast path: read lock.
        {
            let guard = self.inner.read().await;
            if let Some(v) = guard.get(&key) {
                return v.clone();
            }
        }
        // Slow path: write lock.
        let mut guard = self.inner.write().await;
        // Double-check after acquiring write lock.
        if let Some(v) = guard.get(&key) {
            return v.clone();
        }
        let v = f();
        guard.insert(key, v.clone());
        v
    }

    /// Number of entries in the cache.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Whether the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }

    /// Retain only entries for which `f` returns `true`.
    pub async fn retain(&self, f: impl FnMut(&K, &mut V) -> bool + Send) {
        self.inner.write().await.retain(f);
    }

    /// Acquire a read lock and return a clone of the entire map.
    ///
    /// Useful when you need to snapshot all entries (e.g. to iterate
    /// without holding the lock).
    pub async fn snapshot(&self) -> FxHashMap<K, V>
    where
        K: Clone,
        V: Clone,
    {
        self.inner.read().await.clone()
    }

    /// Acquire a write lock and pass the underlying map to `f`.
    ///
    /// Escape hatch for operations not covered by the typed API
    /// (e.g. `should_run_fallback_chart_refresh` which reads + writes
    /// in a single critical section).
    pub async fn with_write<R>(&self, f: impl FnOnce(&mut FxHashMap<K, V>) -> R + Send) -> R {
        let mut guard = self.inner.write().await;
        f(&mut guard)
    }

    /// Acquire a read lock and pass the underlying map to `f`.
    pub async fn with_read<R>(&self, f: impl FnOnce(&FxHashMap<K, V>) -> R + Send) -> R {
        let guard = self.inner.read().await;
        f(&guard)
    }
}

impl<K: Eq + Hash + Send + Sync, V: Send + Sync> Default for Cache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_and_get() {
        let cache: Cache<String, i32> = Cache::new();
        assert!(cache.is_empty().await);

        cache.insert("a".into(), 1).await;
        assert_eq!(cache.get(&"a".into()).await, Some(1));
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn invalidate_removes_entry() {
        let cache: Cache<String, i32> = Cache::new();
        cache.insert("a".into(), 1).await;
        cache.invalidate(&"a".into()).await;
        assert!(cache.get(&"a".into()).await.is_none());
    }

    #[tokio::test]
    async fn clear_removes_all() {
        let cache: Cache<String, i32> = Cache::new();
        cache.insert("a".into(), 1).await;
        cache.insert("b".into(), 2).await;
        cache.clear().await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn get_or_insert_with_returns_existing() {
        let cache: Cache<String, i32> = Cache::new();
        cache.insert("a".into(), 1).await;
        let v = cache.get_or_insert_with("a".into(), || 99).await;
        assert_eq!(v, 1);
    }

    #[tokio::test]
    async fn get_or_insert_with_inserts_missing() {
        let cache: Cache<String, i32> = Cache::new();
        let v = cache.get_or_insert_with("a".into(), || 99).await;
        assert_eq!(v, 99);
        assert_eq!(cache.get(&"a".into()).await, Some(99));
    }

    #[tokio::test]
    async fn retain_filters_entries() {
        let cache: Cache<String, i32> = Cache::new();
        cache.insert("keep".into(), 1).await;
        cache.insert("drop".into(), 2).await;
        cache.retain(|k, _| k == "keep").await;
        assert_eq!(cache.len().await, 1);
        assert!(cache.get(&"keep".into()).await.is_some());
        assert!(cache.get(&"drop".into()).await.is_none());
    }
}
