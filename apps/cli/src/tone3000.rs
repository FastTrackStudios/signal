//! `signal tone3000 …` — the catalog, and the way captures reach the library.
//!
//! Every verb here drives [`signal_tone3000::Tone3000Backend`] directly. The
//! backend is the engine's own, so the session, the image cache and the NAM
//! library are the ones the app uses; nothing is duplicated and nothing has
//! to be re-authorized.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;
use signal_tone3000::{Config, Tone3000Backend};
use signal_tone3000_proto::tone3000::Tone3000 as _;
use signal_tone3000_proto::{ToneQuery, ToneShelf};

/// How long a browser sign-in may take before we stop waiting.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(180);
/// How long one model may take before we call it stuck.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Subcommand)]
pub enum Command {
    /// Whether this machine is configured and signed in.
    Status,
    /// Authorize with TONE3000 in a browser.
    Login,
    /// Forget the local session (a linked account still authorizes).
    Logout,
    /// Sign in to the FastTrackStudio account in a browser. A TONE3000
    /// linked to that account then authorizes downloads here too.
    AccountLogin,
    /// Search the catalog.
    Search {
        /// Free text. Empty browses a shelf instead.
        query: Vec<String>,
        /// `amp`, `amp-cab`, `pedal`, `cab`, `outboard`, `space`.
        #[arg(long)]
        gear: Option<String>,
        /// `best-match`, `newest`, `oldest`, `trending`, `downloads`.
        #[arg(long, default_value = "downloads")]
        sort: String,
        /// `nam` for captures, `ir` for cab impulse responses (`.wav`).
        #[arg(long, default_value = "nam")]
        format: String,
        #[arg(long, default_value_t = 25)]
        limit: u32,
    },
    /// One of the catalog's free, bounded lists.
    Shelf {
        /// `trending`, `latest`, `favourites`, `created`.
        #[arg(default_value = "trending")]
        which: String,
    },
    /// A tone in full, with its models and their ids.
    Show {
        /// Tone id, or the tone's URL.
        tone: String,
    },
    /// Download a tone's models into the local NAM library.
    Fetch {
        /// Tone id, or the tone's URL.
        tone: String,
        /// Fetch only this model (ids come from `show`). Default: all of them.
        #[arg(long)]
        model: Option<String>,
    },
}

pub async fn run(command: Command) -> ExitCode {
    let config_dir = signal_rig_host::store::signal_config_dir();
    let cfg = Config::from_env(
        &config_dir,
        signal_nam::nam_root_from_env(&config_dir.join("nam")),
    );
    let configured = cfg.is_configured();
    let redirect = cfg.redirect_uri.clone();
    let library = cfg.library_root.clone();

    // The same broker the engine builds: a linked FastTrackStudio account
    // authorizes downloads on a machine that never signed in here itself.
    let account = std::sync::Arc::new(signal_account::Account::new(
        signal_account::AccountConfig::from_env(&config_dir),
    ));
    let backend = Tone3000Backend::new(cfg).with_account(account.clone());

    if !configured {
        // A missing key is a different state from being signed out, and it
        // has a different remedy — say which one this is.
        eprintln!(
            "TONE3000 is not configured: no SIGNAL_T3K_PUBLISHABLE_KEY.\n\
             Get one at tone3000.com → Settings → API Keys, then put it in\n\
             `.env` at the repo root (see .env.example)."
        );
        return ExitCode::FAILURE;
    }

    match command {
        Command::Status => status(&backend, &library, &redirect).await,
        Command::Login => login(&backend, &redirect).await,
        Command::AccountLogin => account_login(&account).await,
        Command::Logout => {
            backend.sign_out();
            println!("local session forgotten");
            ExitCode::SUCCESS
        }
        Command::Search {
            query,
            gear,
            sort,
            format,
            limit,
        } => search(&backend, &query.join(" "), gear, &sort, &format, limit).await,
        Command::Shelf { which } => shelf(&backend, &which).await,
        Command::Show { tone } => show(&backend, &tone_id_from(&tone)).await,
        Command::Fetch { tone, model } => {
            fetch(&backend, &library, &tone_id_from(&tone), model.as_deref()).await
        }
    }
}

async fn status(backend: &Tone3000Backend, library: &std::path::Path, redirect: &str) -> ExitCode {
    let status = backend.status().await;
    println!("configured  yes");
    println!("library     {}", library.display());
    println!("redirect    {redirect}");
    if status.signed_in {
        println!("signed in   {} (via {})", status.username, status.via);
    } else if status.error.is_empty() {
        // Never having signed in is not a failure, and must not read as one.
        println!("signed in   no — run `signal tone3000 login`");
    } else {
        println!("signed in   no ({})", status.error);
    }
    ExitCode::SUCCESS
}

async fn search(
    backend: &Tone3000Backend,
    text: &str,
    gear: Option<String>,
    sort: &str,
    format: &str,
    limit: u32,
) -> ExitCode {
    let page = backend
        .search(ToneQuery {
            text: text.to_string(),
            gears: gear.into_iter().collect(),
            format: format.to_string(),
            sort: sort_key(sort),
            page: 1,
            page_size: limit,
        })
        .await;
    if !page.error.is_empty() {
        eprintln!("search failed: {}", page.error);
        return ExitCode::FAILURE;
    }
    println!("{} of {} tones\n", page.tones.len(), page.total);
    for tone in &page.tones {
        print_row(tone);
    }
    ExitCode::SUCCESS
}

async fn shelf(backend: &Tone3000Backend, which: &str) -> ExitCode {
    let page = backend.shelf(shelf_key(which), 1).await;
    if !page.error.is_empty() {
        eprintln!("shelf failed: {}", page.error);
        return ExitCode::FAILURE;
    }
    for tone in &page.tones {
        print_row(tone);
    }
    ExitCode::SUCCESS
}

/// One listing row: the id first, because the id is what the next command
/// takes.
fn print_row(tone: &signal_tone3000_proto::ToneSummary) {
    println!(
        "{:>8}  {:>7}↓  {}",
        tone.id,
        human(tone.downloads_count),
        tone.title
    );
    println!(
        "                   {} · {}",
        blank(&tone.creator, "unknown"),
        blank(&tone.gear, "?")
    );
}

async fn show(backend: &Tone3000Backend, tone_id: &str) -> ExitCode {
    let tone = backend.tone(tone_id.to_string()).await;
    if !tone.error.is_empty() {
        eprintln!("{}", tone.error);
        return ExitCode::FAILURE;
    }
    println!("{}", tone.name);
    println!("  id       {}", tone.id);
    println!("  creator  {}", blank(&tone.creator, "not stated"));
    println!("  licence  {}", blank(&tone.license, "not stated"));
    println!("  gear     {}", blank(&tone.gear, "not stated"));
    if !tone.makes.is_empty() {
        println!("  makes    {}", tone.makes.join(", "));
    }
    if !tone.tags.is_empty() {
        println!("  tags     {}", tone.tags.join(", "));
    }
    println!("  url      {}", tone.tone_url);
    println!("\n  {} model(s):", tone.models.len());
    for model in &tone.models {
        println!(
            "  {:>8}  {}  [{} {}]",
            model.id,
            blank(&model.name, "unnamed"),
            blank(&model.size, "?"),
            blank(&model.architecture, "?")
        );
    }
    ExitCode::SUCCESS
}

/// Authorize in the browser, serving the callback ourselves.
async fn login(backend: &Tone3000Backend, redirect: &str) -> ExitCode {
    if backend.status().await.signed_in {
        println!("already signed in");
        return ExitCode::SUCCESS;
    }
    let request = backend.begin_sign_in(false);
    if request.authorize_url.is_empty() {
        eprintln!("the backend would not start a sign-in");
        return ExitCode::FAILURE;
    }

    let port = port_of(redirect);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            // The engine serves this same path, so this is the one place the
            // CLI and a running engine genuinely contend.
            eprintln!(
                "cannot listen on port {port} ({e}) — a signal engine is probably\n\
                 already running and serving this redirect. Stop it and retry, or\n\
                 sign in from the app instead."
            );
            return ExitCode::FAILURE;
        }
    };

    println!("Opening your browser. Approve the request there.");
    println!("If nothing opens, visit:\n\n{}\n", request.authorize_url);
    open_browser(&request.authorize_url);

    let Some(callback) = wait_for_callback(&listener, SIGN_IN_TIMEOUT) else {
        eprintln!("timed out waiting for the browser callback");
        return ExitCode::FAILURE;
    };
    let (status, _) = backend.complete_from_callback(&callback).await;
    if status.signed_in {
        println!("signed in as {}", status.username);
        ExitCode::SUCCESS
    } else {
        eprintln!("sign-in failed: {}", status.error);
        ExitCode::FAILURE
    }
}

/// Sign in to the FastTrackStudio account, serving its callback ourselves.
async fn account_login(account: &signal_account::Account) -> ExitCode {
    if account.status().signed_in {
        println!("already signed in as {}", account.status().email);
        return ExitCode::SUCCESS;
    }
    let start = account.begin_sign_in();
    let port = port_of(&account.config().redirect_uri);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on port {port} ({e}) — stop the running Signal app and retry");
            return ExitCode::FAILURE;
        }
    };
    println!("Approve the sign-in in your browser. If nothing opens, visit:\n\n{}\n", start.authorize_url);
    if std::env::var_os("SIGNAL_NO_BROWSER").is_none() {
        open_browser(&start.authorize_url);
    }
    let Some(callback) = wait_for_callback(&listener, SIGN_IN_TIMEOUT) else {
        eprintln!("timed out waiting for the browser callback");
        return ExitCode::FAILURE;
    };
    match account.complete_sign_in(&callback).await {
        Ok(status) => {
            println!("signed in as {}", status.email);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sign-in failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Download a tone's models, one at a time, reporting where each landed.
async fn fetch(
    backend: &Tone3000Backend,
    library: &std::path::Path,
    tone_id: &str,
    only: Option<&str>,
) -> ExitCode {
    let tone = backend.tone(tone_id.to_string()).await;
    if !tone.error.is_empty() {
        eprintln!("{}", tone.error);
        return ExitCode::FAILURE;
    }
    let wanted: Vec<_> = match only {
        Some(id) => tone.models.iter().filter(|m| m.id == id).cloned().collect(),
        None => tone.models.clone(),
    };
    if wanted.is_empty() {
        eprintln!("no models matched — the tone lists none, or --model named one it does not have");
        return ExitCode::FAILURE;
    }

    println!("{} — {}", tone.name, blank(&tone.creator, "unknown"));
    let landing = library.join("tone3000").join(&tone.id);
    let mut failed = false;
    for model in &wanted {
        let label = blank(&model.name, "unnamed").to_string();
        // What the directory holds BEFORE asking, so the new file can be
        // told apart from the ones already there. `download_model` returns
        // as soon as the work is queued — that is its contract — so the
        // effect is what there is to wait on.
        //
        // The progress hub would be the direct answer, but the architect
        // this tree pins exposes it only to a vox `Tx`, not to a local
        // subscriber. Watching the library is what `live_check` does for
        // the same reason.
        let before = files_in(&landing);
        backend.download_model(tone.id.clone(), model.id.clone());
        if let Some(path) = wait_for_new_file(&landing, &before, DOWNLOAD_TIMEOUT) {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            println!("  ✓ {label} ({size} bytes)\n    {}", path.display());
        } else {
            // The backend logs the reason at warn level, which the default
            // filter shows — so do not invent one here.
            eprintln!("  ✗ {label}: nothing landed (see the log above)");
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The **model** files a tone's library directory holds right now.
///
/// Only `.nam` and `.wav` — the two library kinds. A download also places the
/// tone's cover next to the model, and a set difference has no order, so
/// counting everything would let `cover.jpg` be reported as the thing that
/// landed. It happened.
fn files_in(dir: &std::path::Path) -> std::collections::HashSet<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "nam" | "wav"))
        })
        .collect()
}

/// Wait for a file that was not in `before` to appear and stop growing.
///
/// Size-stability matters: the bytes are written before the catalog entry,
/// and a path reported mid-write is one the caller may try to load.
fn wait_for_new_file(
    dir: &std::path::Path,
    before: &std::collections::HashSet<std::path::PathBuf>,
    timeout: Duration,
) -> Option<std::path::PathBuf> {
    let start = std::time::Instant::now();
    let mut seen: Option<(std::path::PathBuf, u64)> = None;
    while start.elapsed() < timeout {
        if let Some(path) = files_in(dir).difference(before).next().cloned() {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            // Two polls at the same non-zero size: the write is done.
            if let Some((ref p, last)) = seen
                && *p == path
                && last == size
                && size > 0
            {
                return Some(path);
            }
            seen = Some((path, size));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    seen.map(|(p, _)| p)
}

/// The trailing id of a tone URL, or the argument unchanged.
///
/// The catalog's slugs end in the id (`…-edge-of-breakup-a2-82521`), so a
/// pasted URL works without asking anyone to dig the number out of it.
fn tone_id_from(arg: &str) -> String {
    if !arg.contains('/') {
        return arg.to_string();
    }
    arg.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|slug| slug.rsplit('-').next())
        .unwrap_or(arg)
        .to_string()
}

/// Accept the friendly spelling as well as the API's own.
fn sort_key(sort: &str) -> String {
    match sort {
        "downloads" | "popular" => "downloads-all-time".to_string(),
        other => other.to_string(),
    }
}

fn shelf_key(which: &str) -> ToneShelf {
    match which {
        "latest" | "newest" => ToneShelf::Latest,
        "favourites" | "favorites" | "faves" => ToneShelf::Favorited,
        "created" | "mine" => ToneShelf::Created,
        _ => ToneShelf::Trending,
    }
}

fn port_of(redirect: &str) -> u16 {
    redirect
        .rsplit_once(':')
        .and_then(|(_, rest)| rest.split('/').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or(4040)
}

fn wait_for_callback(listener: &TcpListener, timeout: Duration) -> Option<String> {
    listener.set_nonblocking(true).ok()?;
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
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

const fn blank<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

/// Download counts read better as `35.9K` than as `35847`, and a listing is
/// mostly there to be compared down a column.
fn human(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}K", f64::from(n) / 1000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{human, port_of, sort_key, tone_id_from};

    #[test]
    fn a_pasted_tone_url_yields_its_id() {
        assert_eq!(
            tone_id_from("https://www.tone3000.com/tones/1964-vox-ac30-top-boost-a2-82521"),
            "82521"
        );
        // A trailing slash is what a browser's address bar often hands over.
        assert_eq!(
            tone_id_from("https://www.tone3000.com/tones/plexi-51-51949/"),
            "51949"
        );
        // A bare id passes through untouched.
        assert_eq!(tone_id_from("82521"), "82521");
    }

    #[test]
    fn the_friendly_sort_spelling_maps_to_the_api() {
        assert_eq!(sort_key("downloads"), "downloads-all-time");
        assert_eq!(sort_key("newest"), "newest");
    }

    #[test]
    fn the_redirect_port_is_read_from_the_uri() {
        assert_eq!(port_of("http://localhost:4040/tone3000/callback"), 4040);
        assert_eq!(port_of("http://127.0.0.1:9000/tone3000/callback"), 9000);
        // No port means the default the engine serves.
        assert_eq!(port_of("http://localhost/tone3000/callback"), 4040);
    }

    #[test]
    fn counts_shorten_once_they_stop_being_readable() {
        assert_eq!(human(842), "842");
        assert_eq!(human(35_847), "35.8K");
    }
}
