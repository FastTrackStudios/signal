//! `/demo` — the public playground: the Session app itself, in the live set
//! Task keeps for the demo (a live share link with a reset), everyone who
//! comes in one session with everyone else.
//!
//! The app is the same page a live link opens anywhere (`/app/?live=…`,
//! the `session-daw-web` bundle the image serves under `/app/`); this route
//! only sends the visitor there with the demo's link, baked in at build
//! time (`SESSION_DEMO_LINK`).

use dioxus::prelude::*;

/// The demo set's live share link, when this build has one.
const DEMO_LINK: Option<&str> = option_env!("SESSION_DEMO_LINK");

/// Where the demo opens: the app, in the demo set.
fn demo_url(link: &str) -> String {
    let mut encoded = String::new();
    for b in link.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            encoded.push(char::from(b));
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("/app/?live={encoded}")
}

#[component]
pub fn Demo() -> Element {
    let Some(link) = DEMO_LINK.filter(|l| !l.is_empty()) else {
        return rsx! {
            div { class: "h-screen w-screen flex flex-col items-center justify-center gap-2 bg-zinc-950 text-zinc-100 text-center px-8",
                span { class: "text-xl font-bold", "The live demo opens soon" }
                span { class: "text-sm text-zinc-400 max-w-lg", "This build was made without the demo set's link." }
            }
        };
    };
    let url = demo_url(link);
    // In the browser: go. (The page also carries a plain link to it.)
    use_effect({
        let url = url.clone();
        move || {
            let _ = document::eval(&format!("window.location.replace({url:?})"));
        }
    });
    rsx! {
        // Rendered into the pre-rendered page itself, so the browser goes
        // even before (or without) the site's script running.
        document::Meta { http_equiv: "refresh", content: "0; url={url}" }
        div { class: "h-screen w-screen flex flex-col items-center justify-center gap-3 bg-zinc-950 text-zinc-100 text-center px-8",
            span { class: "text-xl font-bold", "Opening the live demo…" }
            a { class: "text-sm text-sky-400 underline", href: "{url}", "Open it" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::demo_url;

    #[test]
    fn the_demo_link_rides_in_the_apps_address() {
        assert_eq!(
            demo_url("https://task.example/org/days-to-praise/share/abc"),
            "/app/?live=https%3A%2F%2Ftask.example%2Forg%2Fdays-to-praise%2Fshare%2Fabc"
        );
    }
}
