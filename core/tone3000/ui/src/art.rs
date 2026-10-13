//! Tone artwork.
//!
//! The creator's photograph of the rig is the reason this browser is worth
//! looking at rather than reading — a wall of amp names tells you much less
//! than a picture of the amp. It arrives as bytes over the vox link (the
//! engine fetches and caches; see the proto's `ImageData` for why a URL would
//! not do) and is rendered from a `data:` URI.
//!
//! Fetched images are memoised per component tree, so scrolling back to a
//! card does not ask the engine again, and two cards sharing a photograph
//! fetch it once.

use std::collections::HashMap;

use base64::Engine as _;
use dioxus::prelude::*;
use signal_tone3000_proto::tone3000::Tone3000Client;

/// The per-tree image memo: URL → `data:` URI.
#[derive(Clone, Copy)]
pub struct ArtCache(Signal<HashMap<String, String>>);

/// Provide the memo at the root of the browser.
///
/// The signal is created by its own hook before being handed over, for the
/// same reason as the browser's state: an initializer passed to
/// `use_context_provider` runs inside the hook machinery and may not call
/// hooks itself.
pub fn use_art_cache() -> ArtCache {
    let cache = use_signal(HashMap::<String, String>::new);
    use_context_provider(|| ArtCache(cache))
}

/// One tone's picture, or a labelled placeholder when it has none.
///
/// `height` is a CSS length; the image covers its box, because catalog
/// photographs are every aspect ratio there is and a grid of letterboxed
/// cards reads as broken rather than as varied.
#[component]
pub fn ToneArt(url: String, height: String, label: String) -> Element {
    let client = use_hook(try_consume_context::<Tone3000Client>);
    let cache = use_context::<ArtCache>();
    let mut cache_signal = cache.0;

    let data_uri = use_resource(use_reactive!(|(url,)| {
        let client = client.clone();
        async move {
            if url.is_empty() {
                return None;
            }
            if let Some(hit) = cache_signal.read().get(&url) {
                return Some(hit.clone());
            }
            let client = client?;
            let image = client.image(url.clone()).await.ok()?;
            if !image.error.is_empty() || image.bytes.is_empty() {
                return None;
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(&image.bytes);
            let uri = format!("data:{};base64,{encoded}", image.mime);
            cache_signal.write().insert(url, uri.clone());
            Some(uri)
        }
    }));

    // Three states, and they must look different: a picture, a tone that has
    // none, and a fetch still in flight. Collapsing the last two into one
    // makes a slow link look like a catalog full of missing art.
    let resolved = data_uri.read().clone();
    match resolved {
        Some(Some(uri)) => rsx! {
            img {
                src: "{uri}",
                alt: "{label}",
                style: "width:100%;height:{height};object-fit:cover;display:block;
                        border-radius:6px 6px 0 0;background:#1a1a1e;",
            }
        },
        Some(None) => rsx! {
            div {
                style: "width:100%;height:{height};display:flex;align-items:center;
                        justify-content:center;background:linear-gradient(135deg,#26262c,#17171b);
                        border-radius:6px 6px 0 0;color:#6b6b76;font-size:11px;
                        letter-spacing:0.08em;text-transform:uppercase;",
                "no photo"
            }
        },
        None => rsx! {
            div {
                style: "width:100%;height:{height};background:#1e1e23;
                        border-radius:6px 6px 0 0;",
            }
        },
    }
}
