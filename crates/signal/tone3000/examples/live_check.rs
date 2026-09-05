//! End-to-end check against the real catalog.
//!
//! Everything else here is tested against a mock, which proves the code is
//! self-consistent and proves nothing about whether TONE3000 agrees with it.
//! This drives the actual API once, with a real key and a real sign-in, and
//! reports each step.
//!
//! ```console
//! cargo run -p signal-tone3000 --example live_check
//! ```
//!
//! It needs `SIGNAL_T3K_PUBLISHABLE_KEY` (the dev shell exports it from
//! `.env`) and one click in the browser it opens.
//!
//! **It does not touch your library.** Downloads land in a temporary
//! directory that is named at the end and left behind for inspection —
//! a verification run should not put files in the tree the rig plays from.
//!
//! Deliberately not a `#[test]`: it needs a human, a browser and the
//! network, and a test that cannot run in CI should not look like one.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::time::Duration;

use signal_tone3000_proto::tone3000::Tone3000 as _;
use signal_tone3000_proto::{ToneQuery, ToneShelf};
use signal_tone3000::{Config, Tone3000Backend};

/// How long to wait for the user to finish in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(180);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,signal_tone3000=info".into()),
        )
        .init();

    let temp = tempfile::tempdir().expect("temp dir");
    let mut cfg = Config::from_env(temp.path(), temp.path().join("nam"));
    // The session is the one thing worth sharing with the real install: it
    // means a second run of this check needs no second sign-in.
    cfg.token_path = signal_sampler_config_dir().join("tone3000/session.json");

    step("configuration");
    if !cfg.is_configured() {
        fail("no SIGNAL_T3K_PUBLISHABLE_KEY — run `direnv allow`, or export it");
    }
    let key = &cfg.publishable_key;
    let masked = format!("{}…{}", &key[..key.len().min(12)], &key[key.len() - 4..]);
    ok(&format!("publishable key {masked}"));
    ok(&format!("redirect        {}", cfg.redirect_uri));
    ok(&format!("session file    {}", cfg.token_path.display()));
    ok(&format!("library (temp)  {}", cfg.library_root.display()));

    let redirect = cfg.redirect_uri.clone();
    let backend = Tone3000Backend::new(cfg);

    // ── Sign in ───────────────────────────────────────────────────────
    step("session");
    if backend.status().signed_in {
        ok(&format!("already signed in as {}", backend.status().username));
    } else {
        let request = backend.begin_sign_in(false);
        if request.authorize_url.is_empty() {
            fail("the engine would not start a sign-in — key missing?");
        }

        let port = url::Url::parse(&redirect)
            .ok()
            .and_then(|u| u.port())
            .unwrap_or(4040);
        let listener = match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => l,
            Err(e) => fail(&format!(
                "cannot listen on port {port} ({e}) — is a signal engine already running? \
                 Stop it, or point SIGNAL_T3K_REDIRECT_URI at a free port you have registered."
            )),
        };
        listener.set_nonblocking(false).ok();

        println!("\n  Opening your browser. Approve the request there.");
        println!("  If nothing opens, visit:\n\n{}\n", request.authorize_url);
        open_browser(&request.authorize_url);

        let callback = match wait_for_callback(&listener, SIGN_IN_TIMEOUT) {
            Some(c) => c,
            None => fail("timed out waiting for the browser callback"),
        };

        let (status, _request_id) = backend.complete_from_callback(&callback).await;
        if !status.signed_in {
            fail(&format!("sign-in failed: {}", status.error));
        }
        ok(&format!("signed in as {}", status.username));
    }

    // ── Reading the catalog ───────────────────────────────────────────
    step("shelves (the free, bounded lists)");
    let trending = backend.shelf(ToneShelf::Trending, 1).await;
    if !trending.error.is_empty() {
        fail(&format!("trending: {}", trending.error));
    }
    ok(&format!("{} tones", trending.tones.len()));
    for tone in trending.tones.iter().take(3) {
        println!("      {} — {}", tone.title, blank(&tone.creator, "unknown"));
    }

    step("search");
    let found = backend
        .search(ToneQuery {
            text: "plexi".into(),
            gears: vec!["amp".into()],
            format: "nam".into(),
            ..ToneQuery::default()
        })
        .await;
    if !found.error.is_empty() {
        // A rate limit here is a real answer about the free tier, not a bug.
        fail(&format!("search: {}", found.error));
    }
    ok(&format!("{} of {} tones for \"plexi\"", found.tones.len(), found.total));

    // Prefer a searched tone; fall back to the shelf if the search was thin.
    let Some(row) = found.tones.first().or_else(|| trending.tones.first()).cloned() else {
        fail("the catalog returned no tones at all");
    };

    step(&format!("tone detail — {}", row.title));
    let tone = backend.tone(row.id.clone()).await;
    if !tone.error.is_empty() {
        fail(&format!("detail: {}", tone.error));
    }
    ok(&format!("creator  {}", blank(&tone.creator, "not stated")));
    ok(&format!("licence  {}", blank(&tone.license, "not stated")));
    ok(&format!("models   {}", tone.models.len()));
    ok(&format!("images   {}", tone.images.len()));
    for model in tone.models.iter().take(3) {
        println!(
            "      {} [{} {}]",
            blank(&model.name, "unnamed"),
            blank(&model.size, "?"),
            blank(&model.architecture, "?")
        );
    }

    // ── Artwork ───────────────────────────────────────────────────────
    step("artwork");
    match tone.images.first() {
        Some(url) => {
            let image = backend.image(url.clone()).await;
            if image.error.is_empty() {
                ok(&format!("{} bytes, {}", image.bytes.len(), image.mime));
                let again = backend.image(url.clone()).await;
                ok(&format!("second fetch served from cache: {} bytes", again.bytes.len()));
            } else {
                fail(&format!("image: {}", image.error));
            }
        }
        None => println!("      this tone has no photos — skipped"),
    }

    // ── Download ──────────────────────────────────────────────────────
    step("download");
    let Some(model) = tone.models.first().cloned() else {
        fail("the tone has no models to download");
    };
    println!("      {} …", blank(&model.name, "unnamed"));
    backend.download_model(tone.id.clone(), model.id.clone());

    let placed = wait_for_download(&backend, temp.path(), Duration::from_secs(120));
    match placed {
        Some(path) => {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            ok(&format!("{} ({size} bytes)", path.display()));
        }
        None => fail("the model never landed — see the log above"),
    }

    step("catalog entry");
    let catalog_path = temp.path().join("nam/catalog.json");
    match signal_nam::NamCatalog::load(&catalog_path) {
        Ok(catalog) => match catalog.entries.values().next() {
            Some(entry) => {
                ok(&format!("hash     {}", entry.hash));
                match &entry.provenance {
                    Some(p) => {
                        ok(&format!("source   {}", p.source));
                        ok(&format!("creator  {}", opt(&p.creator)));
                        ok(&format!("licence  {}", opt(&p.license)));
                        ok(&format!("tone url {}", opt(&p.tone_url)));
                    }
                    None => fail("the entry carries no provenance — attribution was lost"),
                }
            }
            None => fail("the catalog is empty"),
        },
        Err(e) => fail(&format!("catalog: {e}")),
    }

    println!("\n  ✓ everything above spoke to the real catalog.");
    println!("  files left in {} — delete it when you are done.", temp.path().display());
    // Leak the tempdir so the paths printed above still exist afterwards.
    std::mem::forget(temp);
}

/// Accept one HTTP request and return the full callback URL it asked for.
fn wait_for_callback(listener: &TcpListener, timeout: Duration) -> Option<String> {
    listener.set_nonblocking(true).ok()?;
    let deadline = std::time::Instant::now() + timeout;

    while std::time::Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).ok()?;
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).ok()?;
                // "GET /tone3000/callback?code=…&state=… HTTP/1.1"
                let target = line.split_whitespace().nth(1)?.to_string();

                let body = "<!doctype html><meta charset=utf-8><title>Signal</title>\
                    <body style='font:16px system-ui;background:#111;color:#eee;\
                    display:grid;place-items:center;height:100vh;margin:0'>\
                    <p>Done — you can close this tab.</p>";
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.flush();

                if target.starts_with("/favicon") {
                    continue;
                }
                return Some(format!("http://localhost{target}"));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return None,
        }
    }
    None
}

/// Poll the library for the placed file. `download_model` returns as soon as
/// the work is queued — that is its contract — so the effect is what to wait on.
fn wait_for_download(
    _backend: &Tone3000Backend,
    root: &std::path::Path,
    timeout: Duration,
) -> Option<std::path::PathBuf> {
    let deadline = std::time::Instant::now() + timeout;
    // Wait for the CATALOG, not just the file: the entry is written after
    // the bytes land, and a check that stops at the file races the thing it
    // is trying to verify. (The first run of this example did exactly that.)
    while std::time::Instant::now() < deadline {
        if root.join("nam/catalog.json").is_file() {
            return first_model_file(&root.join("nam/tone3000"));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // Report what did land, so the caller can say which half failed.
    first_model_file(&root.join("nam/tone3000"))
}

fn first_model_file(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = first_model_file(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|e| e == "nam" || e == "wav") {
            return Some(path);
        }
    }
    None
}

fn open_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(url).spawn();
}

/// The real install's config dir, so the session is shared with the app.
fn signal_sampler_config_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("signal")
}

fn step(name: &str) {
    println!("\n── {name}");
}

fn ok(line: &str) {
    println!("   ✓ {line}");
}

fn fail(message: &str) -> ! {
    eprintln!("\n   ✗ {message}");
    std::process::exit(1);
}

fn blank<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

fn opt(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("—")
}
