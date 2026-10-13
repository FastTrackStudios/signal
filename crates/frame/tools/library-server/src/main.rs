//! `frame-library-server` — a folder of surfaces as a live library.
//!
//! ```text
//! frame-library-server DIR [--host 127.0.0.1] [--port 4050] [--http PORT | --no-http] [--no-previews]
//! ```
//! Serves `DIR` (every `.frame` / `.fm` under it, `*.catalog.json` as the
//! engines they fit) through `library-fs`:
//! - `ws://HOST:PORT/vox`: the `SurfaceStore` rpc — list, get, publish,
//!   catalogs, and the `updates(id)` / `changes` streams. The folder is
//!   watched, so an edit on disk (an agent's MCP autosave, an editor)
//!   reaches every open player within a fraction of a second.
//! - `http://HOST:HTTP/` (default PORT+1): `index.json`, downloads at
//!   `surfaces/<id>`, a markup surface's textures and fonts at
//!   `assets/<asset id>`, and thumbnails at `previews/<id>.png`.
//!
//! Previews are rendered on the GPU at startup and on every change
//! (`--no-previews`, or no GPU, leaves them empty unless a `<id>.png`
//! sits next to the surface).

mod http;
mod preview;

use std::path::PathBuf;

use architect::Services as _;
use frame::library::fs::FsLibrary;
use frame::library::fs::SurfaceStore as _;

const USAGE: &str = "usage: frame-library-server DIR [--host 127.0.0.1] [--port 4050] [--http PORT | --no-http] [--no-previews]";

#[derive(Debug, Clone, PartialEq)]
struct Args {
    dir: PathBuf,
    host: String,
    port: u16,
    /// `None` = no HTTP side.
    http: Option<u16>,
    previews: bool,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut dir = None;
    let (mut host, mut port, mut http, mut no_http, mut previews) =
        ("127.0.0.1".to_owned(), 4050u16, None, false, true);
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--host" => host = value()?,
            "--port" => port = value()?.parse().map_err(|_| "--port wants a number")?,
            "--http" => http = Some(value()?.parse().map_err(|_| "--http wants a number")?),
            "--no-http" => no_http = true,
            "--no-previews" => previews = false,
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with('-') => return Err(format!("unknown flag {flag}\n{USAGE}")),
            path if dir.is_none() => dir = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument {extra}\n{USAGE}")),
        }
    }
    let dir = dir.ok_or_else(|| format!("which folder?\n{USAGE}"))?;
    let http = if no_http {
        None
    } else {
        Some(http.unwrap_or(port.saturating_add(1)))
    };
    Ok(Args {
        dir,
        host,
        port,
        http,
        previews,
    })
}

fn run(args: Args) -> Result<(), String> {
    let previewer = if args.previews {
        preview::previewer()
    } else {
        None
    };
    let lib = FsLibrary::open_with(&args.dir, previewer).map_err(|e| e.to_string())?;
    let _watch = lib.watch();
    let count = lib.list(Default::default()).len();
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio: {e}"))?;
    rt.block_on(async move {
        let bind = |port: u16| {
            let addr = format!("{}:{port}", args.host);
            async move { tokio::net::TcpListener::bind(&addr).await.map_err(|e| format!("binding {addr}: {e}")) }
        };
        let ws = bind(args.port).await?;
        eprintln!(
            "frame-library-server: {count} surfaces, {} catalogs from {} at ws://{}:{}/vox (watching for edits)",
            lib.catalogs().len(),
            args.dir.display(),
            args.host,
            args.port
        );
        let http = match args.http {
            Some(p) => {
                let l = bind(p).await?;
                eprintln!("frame-library-server: http://{}:{p}/ (index.json, surfaces/<id>, previews/<id>.png)", args.host);
                Some(l)
            }
            None => None,
        };
        let router = lib.clone().into_router();
        match http {
            Some(h) => {
                tokio::join!(frame::library::link::serve_ws(ws, router), http::serve(h, lib));
            }
            None => frame::library::link::serve_ws(ws, router).await,
        }
        Ok(())
    })
}

fn main() {
    if let Err(e) = parse_args(std::env::args().skip(1)).and_then(run) {
        eprintln!("frame-library-server: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse_args(s.split_whitespace().map(String::from))
    }

    #[test]
    fn defaults_and_flags() {
        let a = args("examples/plugins").unwrap();
        assert_eq!(
            (a.port, a.http, a.previews, a.host.as_str()),
            (4050, Some(4051), true, "127.0.0.1")
        );
        let a = args("--port 5000 d --no-previews --host 0.0.0.0").unwrap();
        assert_eq!((a.port, a.http, a.previews), (5000, Some(5001), false));
        assert_eq!(args("d --http 8080").unwrap().http, Some(8080));
        assert_eq!(args("d --no-http").unwrap().http, None);
        assert!(args("").is_err());
        assert!(args("a b").is_err());
        assert!(args("d --port x").is_err());
    }
}
