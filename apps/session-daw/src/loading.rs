//! The loading screen: what a window or a page shows while a set opens —
//! Session's mark, where the open has got to, and a sweep while it works.
//! One screen for the app (its start screen, `apps/desktop`) and the page
//! (`web_host`), which both open sets the same ways.

use dioxus::prelude::*;

/// Where opening a set has got to: what its loading screen says.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    /// Reaching Task and joining the set — `retry` says why the last try
    /// failed, while it keeps trying.
    Joining { retry: Option<String> },
    /// Bringing the first song's files in.
    Fetching {
        title: String,
        retry: Option<String>,
    },
    /// Opening it into the engine.
    Opening { title: String },
}

/// Session's mark (the site's favicon), as an image source that needs no
/// server: a page and a window draw it the same.
#[must_use]
pub fn mark_src() -> &'static str {
    static SRC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        const SVG: &str = include_str!("../../web/public/favicon.svg");
        let mut uri = String::from("data:image/svg+xml;utf8,");
        for c in SVG.chars() {
            match c {
                '%' => uri.push_str("%25"),
                '#' => uri.push_str("%23"),
                '"' => uri.push('\''),
                '<' => uri.push_str("%3C"),
                '>' => uri.push_str("%3E"),
                '\n' | '\r' => uri.push(' '),
                c => uri.push(c),
            }
        }
        uri
    })
}

/// The loading screen for `progress`.
#[component]
pub fn Loading(progress: Progress) -> Element {
    let (headline, detail, retry) = match progress {
        Progress::Joining { retry } => (
            "Joining the live session".to_owned(),
            "Connecting…".to_owned(),
            retry,
        ),
        Progress::Fetching { title, retry } => (
            format!("Opening {title}"),
            "Bringing the song in…".to_owned(),
            retry,
        ),
        Progress::Opening { title } => (
            format!("Opening {title}"),
            "Laying out the session…".to_owned(),
            None,
        ),
    };
    let detail = retry.unwrap_or(detail);
    rsx! {
        LoadingFrame { headline, detail, failed: false, Sweep {} }
    }
}

/// A thin sweep: working, without claiming how far.
#[component]
pub fn Sweep() -> Element {
    rsx! {
        style { "@keyframes fts-sweep {{ 0% {{ left: -40% }} 100% {{ left: 100% }} }}" }
        div {
            style: "position:relative; width:220px; height:3px; border-radius:2px; overflow:hidden; \
                    background:#1f2228; margin-top:6px;",
            div {
                style: "position:absolute; top:0; left:0; width:40%; height:100%; border-radius:2px; \
                        background:#3aa0ff; animation:fts-sweep 1.4s ease-in-out infinite;",
            }
        }
    }
}

/// The frame every loading state shares: Session's mark, a headline, a
/// line of detail, and whatever goes under it.
#[component]
pub fn LoadingFrame(headline: String, detail: String, failed: bool, children: Element) -> Element {
    let detail_color = if failed { "#f87171" } else { "#9aa0a6" };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100vw; height:100vh; display:flex; \
                    flex-direction:column; align-items:center; justify-content:center; gap:10px; \
                    background:#0f1012; color:#e5e7eb; font-family:system-ui, sans-serif; \
                    text-align:center; padding:0 24px; box-sizing:border-box;",
            img { src: mark_src(), width: "64", height: "64", style: "border-radius:15px; margin-bottom:8px;" }
            div { style: "font-size:18px; font-weight:650;", "{headline}" }
            div { style: "font-size:13px; color:{detail_color}; max-width:420px; line-height:1.5;", "{detail}" }
            {children}
        }
    }
}
