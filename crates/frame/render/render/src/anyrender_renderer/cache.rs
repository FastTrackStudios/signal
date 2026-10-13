use super::geometry::compute_svg_fit_transform;
use std::sync::Arc;

/// Cache for parsed SVG path strings → BezPath. Keyed by string hash.
/// Returns an Arc to avoid cloning the path on every frame.
pub(super) fn cached_bez_path(path_data: &str) -> Option<Arc<kurbo::BezPath>> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, Arc<kurbo::BezPath>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path_data.hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return Some(Arc::clone(cached));
        }
    }

    let path = kurbo::BezPath::from_svg(path_data).ok()?;
    let arc = Arc::new(path);

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, Arc::clone(&arc));
    }

    Some(arc)
}

/// Cache for base64-encoded SVG → decoded text string.
/// Returns an Arc to avoid cloning the string on every frame.
pub(super) fn cached_svg_decode(svg_base64: &str) -> Option<Arc<String>> {
    use base64::Engine;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, Arc<String>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    svg_base64.hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return Some(Arc::clone(cached));
        }
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(svg_base64)
        .ok()?;
    let text = Arc::new(String::from_utf8(bytes).ok()?);

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, Arc::clone(&text));
    }

    Some(text)
}

/// Cache for parsed usvg::Tree objects. Avoids XML parsing on every frame.
/// This is the highest-impact cache — each render_svg_str call was parsing
/// the full SVG XML, and blurred SVGs parsed 6-20x per element per frame.
pub(super) fn cached_svg_tree(svg_text: &str) -> Option<Arc<anyrender_svg::usvg::Tree>> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, Arc<anyrender_svg::usvg::Tree>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    svg_text.hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return Some(Arc::clone(cached));
        }
    }

    let opt = anyrender_svg::usvg::Options::default();
    let tree = anyrender_svg::usvg::Tree::from_str(svg_text, &opt).ok()?;
    let arc = Arc::new(tree);

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, Arc::clone(&arc));
    }

    Some(arc)
}

/// Cache for SVG text → fit transform. Uses cached_svg_tree to avoid re-parsing.
pub(super) fn cached_svg_fit_transform(
    svg_text: &str,
    target_width: f64,
    target_height: f64,
) -> kurbo::Affine {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, kurbo::Affine>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    svg_text.hash(&mut hasher);
    target_width.to_bits().hash(&mut hasher);
    target_height.to_bits().hash(&mut hasher);
    let key = hasher.finish();

    if let Ok(cache) = CACHE.lock() {
        if let Some(cached) = cache.get(&key) {
            return *cached;
        }
    }

    let transform = compute_svg_fit_transform(svg_text, target_width, target_height);

    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, transform);
    }

    transform
}
