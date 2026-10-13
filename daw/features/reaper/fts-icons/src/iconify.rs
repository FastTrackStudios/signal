use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;

const API: &str = "https://api.iconify.design";

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("fts-icons")
}

/// Search the Iconify collection. Returns `prefix:name` ids.
pub fn search(query: &str, limit: u32) -> Result<Vec<String>> {
    let url = format!("{API}/search?query={}&limit={limit}", urlencode(query));
    let body: serde_json::Value = ureq::get(&url)
        .call()
        .with_context(|| format!("iconify search failed: {url}"))?
        .into_json()?;
    let icons = body["icons"]
        .as_array()
        .context("malformed search response")?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    Ok(icons)
}

/// Fetch raw SVG for `prefix:name`, with on-disk cache.
pub fn fetch_svg(id: &str) -> Result<String> {
    let (prefix, name) = id
        .split_once(':')
        .with_context(|| format!("icon id {id:?} must be prefix:name (e.g. mdi:eye)"))?;
    let cache = cache_dir().join(prefix).join(format!("{name}.svg"));
    if let Ok(svg) = fs::read_to_string(&cache) {
        return Ok(svg);
    }
    let url = format!("{API}/{prefix}/{name}.svg");
    let resp = ureq::get(&url)
        .call()
        .with_context(|| format!("fetch failed: {url}"))?;
    let svg = resp.into_string()?;
    if svg.trim() == "404" || !svg.contains("<svg") {
        bail!("icon {id:?} not found on iconify");
    }
    fs::create_dir_all(cache.parent().unwrap())?;
    fs::write(&cache, &svg)?;
    Ok(svg)
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
