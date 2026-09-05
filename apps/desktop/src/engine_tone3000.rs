//! `engine_tone3000.rs` — the OAuth landing strip.
//!
//! TONE3000's authorization page is a web application, so it runs in the
//! user's own browser and redirects when it is done. Something has to be
//! listening at the other end of that redirect, and in this product the only
//! thing that can be is the engine: the desktop GUI renders through Blitz
//! (a renderer, not a browser), the plugin editor likewise, and the browser
//! remote may be on a different device entirely.
//!
//! So the engine serves the registered redirect URI itself, on the HTTP
//! server it already runs:
//!
//! ```text
//! GET /tone3000/callback?code=…&state=…[&tone_id=…]
//! ```
//!
//! The route hands the whole callback to the backend, which checks the nonce
//! and exchanges the code, and then renders a page telling the user to go
//! back to the app. Whichever GUI started the flow is watching `status()` and
//! picks the session up from there — it never sees the code, and never needs
//! to.

use architect::host::EngineHost;
use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use signal_tone3000::Tone3000Backend;

/// Mount the FastTrackStudio account's callback route.
///
/// Same argument as the TONE3000 one below, one level up: the issuer's
/// login page is a web application, so the engine catches the redirect and
/// whichever GUI started the flow picks the session up from `status()`.
pub fn extend_account(host: EngineHost, account: std::sync::Arc<signal_account::Account>) -> EngineHost {
    let path = callback_path(&account.config().redirect_uri);
    tracing::info!(path, issuer = account.config().issuer, "account: callback route mounted");
    host.extend(
        Router::new()
            .route(&path, get(account_callback))
            .with_state(account),
    )
}

async fn account_callback(
    State(account): State<std::sync::Arc<signal_account::Account>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Html<String> {
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("{}?{query}", account.config().redirect_uri);

    match account.complete_sign_in(&url).await {
        Ok(status) => {
            let who = if status.email.is_empty() {
                "You can close this tab and go back to Signal.".to_string()
            } else {
                format!(
                    "Signed in as {}. You can close this tab and go back to Signal.",
                    html_escape(&status.email)
                )
            };
            Html(page("Signed in to FastTrackStudio", &who))
        }
        Err(e) => Html(page(
            "Sign-in did not complete",
            &html_escape(&e.to_string()),
        )),
    }
}

/// Mount the callback route on the engine's HTTP server.
pub fn extend(host: EngineHost, backend: Tone3000Backend) -> EngineHost {
    let path = callback_path(backend.redirect_uri());
    tracing::info!(path, "tone3000: callback route mounted");
    host.extend(
        Router::new()
            .route(&path, get(callback))
            .with_state(backend),
    )
}

/// The path component of the configured redirect URI.
///
/// Taken from the URI rather than hard-coded, because the URI is what is
/// registered with TONE3000 and the two must agree: a redirect we do not
/// serve is a sign-in that dead-ends in the browser.
fn callback_path(redirect_uri: &str) -> String {
    url::Url::parse(redirect_uri)
        .ok()
        .map(|u| u.path().to_string())
        .filter(|p| p.starts_with('/') && p.len() > 1)
        .unwrap_or_else(|| signal_tone3000::config::CALLBACK_PATH.to_string())
}

async fn callback(
    State(backend): State<Tone3000Backend>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Html<String> {
    // Rebuild the URL the backend expects. The query is what matters; the
    // origin is the one we registered.
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("{}?{query}", backend.redirect_uri());

    let (status, _request_id) = backend.complete_from_callback(&url).await;
    if status.signed_in {
        Html(page(
            "Signed in to TONE3000",
            &format!(
                "Signed in as {}. You can close this tab and go back to Signal.",
                html_escape(&status.username)
            ),
        ))
    } else if status.error == "sign-in was canceled" {
        // Not a failure page: the user closed the picker on purpose.
        Html(page(
            "No tone selected",
            "You can close this tab and go back to Signal.",
        ))
    } else {
        // The provider's own words, not a generic apology: "access_denied"
        // and "this link has expired" call for different actions.
        Html(page("Sign-in did not complete", &html_escape(&status.error)))
    }
}

/// The landing page. Deliberately one self-contained document with no assets:
/// it is served once, to a browser that is about to be closed.
fn page(heading: &str, detail: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>{heading}</title>\
         <style>body{{font:16px/1.5 system-ui,sans-serif;margin:0;display:grid;\
         place-items:center;min-height:100vh;background:#111;color:#eee}}\
         main{{max-width:32rem;padding:2rem;text-align:center}}\
         h1{{font-size:1.25rem;margin:0 0 .5rem}}p{{margin:0;opacity:.75}}</style>\
         <main><h1>{heading}</h1><p>{detail}</p></main>"
    )
}

fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_route_follows_the_registered_redirect_uri() {
        assert_eq!(
            callback_path("http://127.0.0.1:4040/tone3000/callback"),
            "/tone3000/callback"
        );
        assert_eq!(
            callback_path("https://rig.local:4040/oauth/t3k"),
            "/oauth/t3k"
        );
    }

    /// A custom-scheme redirect (iOS) has no path we can serve; the engine
    /// still needs a route, and the default is the one it advertises.
    #[test]
    fn a_redirect_without_a_usable_path_falls_back() {
        assert_eq!(
            callback_path("fasttrackstudio://t3k"),
            signal_tone3000::config::CALLBACK_PATH
        );
        assert_eq!(
            callback_path("not a url"),
            signal_tone3000::config::CALLBACK_PATH
        );
    }

    /// The provider's error text lands in HTML, so it is escaped — the
    /// content comes from a query string a browser was pointed at.
    #[test]
    fn callback_text_is_escaped() {
        let rendered = page("t", &html_escape("<script>alert(1)</script>"));
        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;"));
    }

    #[test]
    fn query_values_are_re_encoded() {
        assert_eq!(urlencode("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(urlencode("plain-value_1.0~"), "plain-value_1.0~");
    }
}
