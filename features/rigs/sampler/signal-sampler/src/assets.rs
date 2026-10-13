//! In-memory rig assets — `.nam` models and cab IRs by the path a block
//! names them with.
//!
//! The browser has no filesystem: a web rig fetches its models and IRs over
//! HTTP (or from OPFS) and installs the bytes here under the same key the
//! rig's config uses, and [`crate::rig::build_block`] reads them from here
//! before it would touch a disk. Natively the registry is empty unless a
//! host fills it, and blocks load from their paths as always.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

fn registry() -> &'static RwLock<HashMap<String, Arc<[u8]>>> {
    static R: OnceLock<RwLock<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Install `bytes` under `key` (replacing any previous bytes).
pub fn install(key: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
    registry()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key.into(), bytes.into());
}

/// The bytes installed under `key`, if any.
#[must_use]
pub fn get(key: &str) -> Option<Arc<[u8]>> {
    registry()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
        .cloned()
}

/// Whether `key` has bytes installed.
#[must_use]
pub fn contains(key: &str) -> bool {
    registry()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(key)
}

/// Every installed key — what a web rig has already fetched.
#[must_use]
pub fn keys() -> Vec<String> {
    registry()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .keys()
        .cloned()
        .collect()
}

/// Forget everything installed.
pub fn clear() {
    registry()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// The display stem of an asset key (`…/Deluxe Reverb.nam` → `Deluxe Reverb`).
#[must_use]
pub fn stem(key: &str) -> String {
    let file = key.rsplit(['/', '\\']).next().unwrap_or(key);
    file.rsplit_once('.').map_or(file, |(s, _)| s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_bytes_come_back_by_key() {
        install("test/amp.nam", vec![1u8, 2, 3]);
        assert_eq!(get("test/amp.nam").as_deref(), Some(&[1u8, 2, 3][..]));
        assert!(get("test/other.nam").is_none());
        assert_eq!(stem("x/y/Deluxe Reverb.nam"), "Deluxe Reverb");
    }
}
