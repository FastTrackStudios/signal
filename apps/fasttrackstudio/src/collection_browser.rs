//! Collection browser — a standalone web surface at `/{org}/{collection}`.
//!
//! Additive to the normal app: when the browser lands on a two-segment
//! path (e.g. `/days-to-praise/church-worship`), the wasm entry launches
//! this root INSTEAD of the shell (`main::launch_app`). Every other URL is
//! the app exactly as before.
//!
//! Collections live on the **task-server**, not the fts signal engine at
//! :4040. The task-server exposes one `CollectionService` per org on its
//! per-org router at `/org/{slug}/vox` (see apps/task/server/src/lib.rs).
//! So this view dials `<task-server base>/org/{org}/vox`, `list`s that
//! org's collections, and picks the one whose title slug matches the URL.
//!
//! Configure the task-server base with the `task-server-ws-url` pref
//! (localStorage `fts.task-server-ws-url`); default `ws://127.0.0.1:18080`.

use collection_proto::{Collection, CollectionItem, CollectionServiceClient};
use dioxus::prelude::*;

use crate::prefs;
use crate::remote::{EngineTarget, establish};

/// The task-server base WebSocket URL (scheme + host, no path). Read from
/// the `task-server-ws-url` pref, default `ws://127.0.0.1:18080`. Any
/// trailing `/vox` or `/` is stripped so the per-org path composes cleanly.
fn task_server_base() -> String {
    let raw = prefs::get("task-server-ws-url")
        .unwrap_or_else(|| "ws://127.0.0.1:18080".to_string());
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_suffix("/vox").unwrap_or(trimmed);
    trimmed.trim_end_matches('/').to_string()
}

/// The per-org vox URL for `org` on the configured task-server.
fn org_vox_url(org: &str) -> String {
    format!("{}/org/{}/vox", task_server_base(), org)
}

/// A URL-safe slug: lowercase, non-alphanumerics collapsed to single
/// dashes, edges trimmed. Used to match a collection's human title against
/// the `{collection}` path segment case-insensitively.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = true; // trims leading dashes
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// The `{org}`/`{collection}` segments of the current URL path, if the path
/// is exactly two non-empty segments. Anything else (root, one segment,
/// three+) is NOT a collection route — the normal app handles it.
#[cfg(target_arch = "wasm32")]
fn path_segments() -> Option<(String, String)> {
    let path = web_sys::window()?.location().pathname().ok()?;
    let mut segs = path.split('/').filter(|s| !s.is_empty());
    let org = segs.next()?.to_string();
    let collection = segs.next()?.to_string();
    if segs.next().is_some() {
        return None; // three or more segments — not us
    }
    // Percent-decoding is unlikely to matter for these slugs; keep raw.
    Some((org, collection))
}

/// True when the current URL is a `/{org}/{collection}` route, i.e. the
/// wasm entry should launch [`CollectionBrowser`] instead of the app shell.
#[cfg(target_arch = "wasm32")]
pub(crate) fn route_matches() -> bool {
    path_segments().is_some()
}

/// A prettier label for a song-ish node id: `keep-on-finding-more` →
/// `Keep On Finding More`. Falls back to the raw id for anything else.
fn pretty_label(item: &CollectionItem) -> String {
    let id = &item.node.id;
    if id.is_empty() {
        return item.node.kind.as_str().to_string();
    }
    id.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Loaded/resolved state for the requested collection.
enum Loaded {
    /// Resolved the collection for this slug.
    Found(Collection),
    /// Connected + listed, but no collection matched the slug.
    NotFound,
}

/// The collection browser root. Takes no props — it reads the URL itself,
/// dials the task-server, and renders the matching collection's ordered
/// songs.
#[component]
pub fn CollectionBrowser() -> Element {
    #[cfg(target_arch = "wasm32")]
    let route = path_segments();
    #[cfg(not(target_arch = "wasm32"))]
    let route: Option<(String, String)> = None;

    let (org, slug) = match route {
        Some(pair) => pair,
        // Should not happen (the entry only launches us on a match), but
        // render a clear message rather than panic.
        None => {
            return rsx! {
                Shell {
                    div { style: "color:#a1a1aa; font-size:14px;",
                        "No collection in the URL. Expected /{{org}}/{{collection}}."
                    }
                }
            };
        }
    };

    // async: connect → list → resolve by slug. `Result<Loaded, String>`;
    // the `Err` string is a human message for the error panel.
    let org_r = org.clone();
    let slug_r = slug.clone();
    let data = use_resource(move || {
        let org = org_r.clone();
        let slug = slug_r.clone();
        async move {
            let url = org_vox_url(&org);
            let target = EngineTarget::Ws(url.clone());
            let client = establish::<CollectionServiceClient>(&target)
                .await
                .ok_or_else(|| format!("could not reach the task-server at {url}"))?;
            let mut collections = client
                .list(org.clone(), None)
                .await
                .map_err(|e| format!("task-server error: {e}"))?;
            let want = slugify(&slug);
            match collections
                .iter()
                .position(|c| slugify(&c.title) == want || c.title.eq_ignore_ascii_case(&slug))
            {
                Some(i) => {
                    let mut col = collections.swap_remove(i);
                    col.sort_items(); // canonical ascending-by-rank order
                    Ok::<Loaded, String>(Loaded::Found(col))
                }
                None => Ok(Loaded::NotFound),
            }
        }
    });

    let body = match &*data.read_unchecked() {
        None => rsx! {
            div { style: "color:#a1a1aa; font-size:14px;",
                "Connecting to the task-server…"
            }
        },
        Some(Err(msg)) => rsx! {
            div { style: "display:flex; flex-direction:column; gap:8px;",
                span { style: "color:#ef4444; font-size:14px; font-weight:600;", "Could not load collection" }
                span { style: "color:#a1a1aa; font-size:13px;", "{msg}" }
                span { style: "color:#52525b; font-size:12px;",
                    "Set task-server-ws-url in localStorage (key fts.task-server-ws-url) to point elsewhere."
                }
            }
        },
        Some(Ok(Loaded::NotFound)) => rsx! {
            div { style: "display:flex; flex-direction:column; gap:6px;",
                span { style: "color:#e4e4e7; font-size:15px; font-weight:600;", "Collection not found" }
                span { style: "color:#a1a1aa; font-size:13px;",
                    "No collection titled \"{slug}\" in org \"{org}\"."
                }
            }
        },
        Some(Ok(Loaded::Found(col))) => rsx! {
            CollectionCard { collection: col.clone() }
        },
    };

    rsx! {
        Shell {
            div { style: "font-size:11px; letter-spacing:1px; color:#52525b; text-transform:uppercase;", "{org}" }
            {body}
        }
    }
}

/// The resolved collection: title, kind badge, and an ordered song list.
#[component]
fn CollectionCard(collection: Collection) -> Element {
    let items = collection.items.clone();
    rsx! {
        div { style: "display:flex; align-items:baseline; gap:10px;",
            span { style: "font-size:22px; font-weight:700; color:#e4e4e7;", "{collection.title}" }
            span {
                style: "font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:1px; color:#a1a1aa; border:1px solid #27272a; border-radius:999px; padding:2px 8px;",
                "{collection.kind.as_str()}"
            }
        }
        if items.is_empty() {
            div { style: "color:#71717a; font-size:14px; margin-top:6px;", "No songs yet." }
        } else {
            ol { style: "list-style:none; margin:0; padding:0; display:flex; flex-direction:column; gap:6px; margin-top:6px;",
                for (i, item) in items.iter().enumerate() {
                    li {
                        key: "{item.node.to_token()}",
                        style: "display:flex; align-items:baseline; gap:12px; padding:10px 12px; border:1px solid #27272a; border-radius:8px; background:#111113;",
                        span { style: "font-size:12px; color:#52525b; min-width:22px; text-align:right;", "{i + 1}" }
                        div { style: "display:flex; flex-direction:column; gap:2px;",
                            span { style: "font-size:14px; color:#e4e4e7; font-weight:500;", "{pretty_label(item)}" }
                            span { style: "font-size:11px; color:#52525b;", "{item.node.to_token()}" }
                        }
                    }
                }
            }
        }
    }
}

/// The page frame: a dark full-viewport column with the same reset the app
/// shell uses, so this surface matches FastTrackStudio's look.
#[component]
fn Shell(children: Element) -> Element {
    rsx! {
        document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;}*{box-sizing:border-box;}"} }
        div {
            style: "min-height:100vh; background:#0a0a0a; color:#e4e4e7; font-family:sans-serif; display:flex; flex-direction:column; align-items:center; padding:48px 16px;",
            div { style: "width:100%; max-width:640px; display:flex; flex-direction:column; gap:14px;",
                header { style: "display:flex; align-items:center; gap:8px; border-bottom:1px solid #27272a; padding-bottom:10px; margin-bottom:4px;",
                    span { style: "font-weight:700; letter-spacing:1px; font-size:12px; color:#a1a1aa;", "FASTTRACKSTUDIO" }
                }
                {children}
            }
        }
    }
}
